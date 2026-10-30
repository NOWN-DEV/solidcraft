#!/usr/bin/env python3
"""Upstream relay: runs the request files a push adds under requests/ against the
upstream repositories, as the account that owns UPSTREAM_TOKEN.

One request = one JSON file. See README.md for the fields of each action.
Results go to results/<request name>.json and to the job log.
"""

import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

API = "https://api.github.com"
ALLOWED_REPOS = {"tangency-labs/solidcraft", "tangency-labs/solidcore"}
WRITE_ACTIONS = {
    "pr_create", "issue_create", "comment", "review_reply",
    "edit", "edit_comment", "delete_comment", "close", "reopen", "fork", "ready",
}
READ_ACTIONS = {"read", "list", "search"}
MAX_BODY = 65000
MAX_TITLE = 256
MAX_PAGES = 5
REQUEST_DIR = "requests"
RESULT_DIR = "results"


class RelayError(Exception):
    pass


class GitHub:
    def __init__(self, token):
        self.token = token

    def call(self, method, path, data=None, query=None):
        url = API + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        body = json.dumps(data).encode() if data is not None else None
        req = urllib.request.Request(url, data=body, method=method)
        req.add_header("Authorization", "Bearer " + self.token)
        req.add_header("Accept", "application/vnd.github+json")
        req.add_header("X-GitHub-Api-Version", "2022-11-28")
        if body is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                raw = resp.read()
        except urllib.error.HTTPError as e:
            detail = e.read().decode(errors="replace")[:2000]
            raise RelayError(f"GitHub {method} {path}: HTTP {e.code}: {detail}") from None
        return json.loads(raw) if raw else None

    def pages(self, path, query=None):
        out = []
        for page in range(1, MAX_PAGES + 1):
            q = dict(query or {}, per_page=100, page=page)
            items = self.call("GET", path, query=q)
            if isinstance(items, dict):  # check-runs wraps its list
                items = items.get("check_runs", [])
            out.extend(items)
            if len(items) < 100:
                break
        return out


def added_request_files(event, compare=None):
    """Request files this push added, in name order.

    Actions push events do not list the files of each commit, so the push is
    compared (before...after) through the API; a new branch runs nothing.
    """
    def wanted(path):
        return path.startswith(REQUEST_DIR + "/") and path.endswith(".json")

    files = set()
    for commit in event.get("commits") or []:
        files.update(p for p in commit.get("added") or [] if wanted(p))
    before, after = event.get("before", ""), event.get("after", "")
    if compare and before and after and set(before) != {"0"}:
        files.update(f["filename"] for f in compare(before, after)
                     if f.get("status") == "added" and wanted(f.get("filename", "")))
    return sorted(files)


def load_request(path, root="."):
    with open(os.path.join(root, path), encoding="utf-8") as f:
        req = json.load(f)
    if not isinstance(req, dict):
        raise RelayError("a request is a JSON object")
    body_file = req.pop("body_file", None)
    if body_file is not None:
        if "body" in req:
            raise RelayError("give body or body_file, not both")
        base = os.path.realpath(os.path.join(root, REQUEST_DIR))
        target = os.path.realpath(os.path.join(root, os.path.dirname(path), str(body_file)))
        if not target.startswith(base + os.sep):
            raise RelayError("body_file must be inside requests/")
        with open(target, encoding="utf-8") as f:
            req["body"] = f.read()
    return req


def validate(req, deny):
    action = req.get("action")
    if action not in WRITE_ACTIONS | READ_ACTIONS:
        raise RelayError(f"unknown action {action!r}")
    if req.get("repo") not in ALLOWED_REPOS:
        raise RelayError(f"repo must be one of {sorted(ALLOWED_REPOS)}")
    if not str(req.get("requested_by") or "").strip():
        raise RelayError("requested_by is required (who asked for this, and the session)")
    for key in ("number", "comment_id"):
        if key in req and (not isinstance(req[key], int) or isinstance(req[key], bool) or req[key] <= 0):
            raise RelayError(f"{key} must be a positive integer")
    body = req.get("body")
    if body is not None and (not isinstance(body, str) or len(body) > MAX_BODY):
        raise RelayError(f"body must be text of at most {MAX_BODY} characters")
    title = req.get("title")
    if title is not None and (not isinstance(title, str) or not title.strip() or len(title) > MAX_TITLE):
        raise RelayError(f"title must be non-empty text of at most {MAX_TITLE} characters")
    if action in WRITE_ACTIONS:
        text = "\n".join(str(req.get(k) or "") for k in ("title", "body", "head", "base"))
        if deny.search(text):
            raise RelayError("refused by the privacy pattern (RELAY_DENY_PATTERN); nothing was posted")
    need = {
        "pr_create": ("title", "head", "base"),
        "issue_create": ("title", "body"),
        "comment": ("number", "body"),
        "review_reply": ("number", "comment_id", "body"),
        "edit": ("number",),
        "edit_comment": ("comment_id", "body"),
        "delete_comment": ("comment_id",),
        "close": ("number",),
        "reopen": ("number",),
        "fork": (),
        "ready": ("number",),
        "read": ("number",),
        "list": (),
        "search": ("query",),
    }[action]
    missing = [k for k in need if req.get(k) in (None, "")]
    if missing:
        raise RelayError(f"{action} needs {', '.join(missing)}")
    if action == "edit" and req.get("title") is None and req.get("body") is None:
        raise RelayError("edit needs title or body")


def require_own(item, login, what):
    owner = (item.get("user") or {}).get("login", "")
    if owner.lower() != login.lower():
        raise RelayError(f"{what} belongs to {owner!r}, not {login!r}; the relay only changes its own")


def comment_path(repo, req):
    kind = req.get("comment_kind", "issue")
    if kind == "issue":
        return f"/repos/{repo}/issues/comments/{req['comment_id']}"
    if kind == "review":
        return f"/repos/{repo}/pulls/comments/{req['comment_id']}"
    raise RelayError("comment_kind is issue (conversation) or review (code line)")


def slim(item, keys):
    return {k: item.get(k) for k in keys if k in item}


def run(gh, req, login, dry_run=False):
    repo, action = req["repo"], req["action"]
    n = req.get("number")

    if action == "read":
        issue = gh.call("GET", f"/repos/{repo}/issues/{n}")
        out = {"item": slim(issue, ("number", "title", "state", "html_url", "body", "created_at", "updated_at")),
               "author": (issue.get("user") or {}).get("login"),
               "comments": [dict(slim(c, ("id", "body", "created_at", "html_url")), author=(c.get("user") or {}).get("login"))
                            for c in gh.pages(f"/repos/{repo}/issues/{n}/comments")]}
        if "pull_request" in issue:
            pr = gh.call("GET", f"/repos/{repo}/pulls/{n}")
            sha = pr["head"]["sha"]
            out["pull"] = {"head": pr["head"].get("label"), "head_sha": sha, "base": pr["base"].get("ref"),
                           "draft": pr.get("draft"), "merged": pr.get("merged"), "mergeable": pr.get("mergeable"),
                           "mergeable_state": pr.get("mergeable_state")}
            out["reviews"] = [dict(slim(r, ("id", "state", "body", "submitted_at")), author=(r.get("user") or {}).get("login"))
                              for r in gh.pages(f"/repos/{repo}/pulls/{n}/reviews")]
            out["review_comments"] = [dict(slim(c, ("id", "in_reply_to_id", "path", "line", "body", "created_at", "html_url")),
                                           author=(c.get("user") or {}).get("login"))
                                      for c in gh.pages(f"/repos/{repo}/pulls/{n}/comments")]
            out["checks"] = [dict(slim(c, ("name", "status", "conclusion", "html_url", "completed_at")),
                                  **({"output": slim(c["output"], ("title", "summary"))}
                                     if c.get("conclusion") == "failure" and c.get("output") else {}))
                             for c in gh.pages(f"/repos/{repo}/commits/{sha}/check-runs")]
        return out

    if action == "list":
        state = req.get("state", "open")
        items = gh.pages(f"/repos/{repo}/issues", query={"creator": login, "state": state})
        return {"items": [dict(slim(i, ("number", "title", "state", "updated_at", "comments", "html_url")),
                               is_pr="pull_request" in i) for i in items]}

    if action == "search":
        q = f"repo:{repo} {req['query']}"
        res = gh.call("GET", "/search/issues", query={"q": q, "per_page": 50})
        return {"total": res.get("total_count"),
                "items": [dict(slim(i, ("number", "title", "state", "updated_at", "html_url")),
                               is_pr="pull_request" in i, author=(i.get("user") or {}).get("login"))
                          for i in res.get("items", [])]}

    # Writes from here on.
    if action == "pr_create":
        head = str(req["head"])
        if ":" not in head or head.split(":", 1)[0].lower() != login.lower():
            raise RelayError(f"head must be '{login}:<branch>' (a branch on our fork)")
        call = ("POST", f"/repos/{repo}/pulls",
                {"title": req["title"], "head": head, "base": req["base"], "body": req.get("body", ""),
                 "draft": bool(req.get("draft", True)), "maintainer_can_modify": True})
    elif action == "ready":
        pr = gh.call("GET", f"/repos/{repo}/pulls/{n}")
        require_own(pr, login, f"#{n}")
        mutation = ("mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) "
                    "{ pullRequest { isDraft } } }")
        call = ("POST", "/graphql", {"query": mutation, "variables": {"id": pr["node_id"]}})
    elif action == "fork":
        call = ("POST", f"/repos/{repo}/forks", {"default_branch_only": True})
    elif action == "issue_create":
        call = ("POST", f"/repos/{repo}/issues", {"title": req["title"], "body": req["body"]})
    elif action == "comment":
        call = ("POST", f"/repos/{repo}/issues/{n}/comments", {"body": req["body"]})
    elif action == "review_reply":
        call = ("POST", f"/repos/{repo}/pulls/{n}/comments/{req['comment_id']}/replies", {"body": req["body"]})
    elif action in ("edit", "close", "reopen"):
        issue = gh.call("GET", f"/repos/{repo}/issues/{n}")
        require_own(issue, login, f"#{n}")
        if action == "edit":
            data = {k: req[k] for k in ("title", "body") if req.get(k) is not None}
        else:
            data = {"state": "closed" if action == "close" else "open"}
        kind = "pulls" if "pull_request" in issue else "issues"
        call = ("PATCH", f"/repos/{repo}/{kind}/{n}", data)
    elif action in ("edit_comment", "delete_comment"):
        path = comment_path(repo, req)
        require_own(gh.call("GET", path), login, f"comment {req['comment_id']}")
        call = ("PATCH", path, {"body": req["body"]}) if action == "edit_comment" else ("DELETE", path, None)
    else:  # validate() already refused anything else
        raise RelayError(f"unknown action {action!r}")

    if dry_run:
        return {"dry_run": True, "would_call": {"method": call[0], "path": call[1], "data": call[2]}}
    res = gh.call(*call)
    return {"done": True, "method": call[0], "path": call[1],
            "result": slim(res or {}, ("number", "id", "html_url", "state", "draft"))}


def main(argv):
    event_path = argv[1] if len(argv) > 1 else os.environ.get("GITHUB_EVENT_PATH", "")
    with open(event_path, encoding="utf-8") as f:
        event = json.load(f)
    token = os.environ.get("UPSTREAM_TOKEN", "")
    pattern = os.environ.get("RELAY_DENY_PATTERN", "")
    if not token or not pattern:
        print("UPSTREAM_TOKEN and RELAY_DENY_PATTERN must both be set (environment 'upstream'); nothing was run.")
        return 1
    deny = re.compile(pattern, re.IGNORECASE)
    gh = GitHub(token)
    login = gh.call("GET", "/user")["login"]
    os.makedirs(RESULT_DIR, exist_ok=True)
    own = GitHub(os.environ.get("GITHUB_TOKEN", ""))
    repo_self = os.environ.get("GITHUB_REPOSITORY", "")

    def compare(before, after):
        return own.call("GET", f"/repos/{repo_self}/compare/{before}...{after}").get("files") or []

    files = added_request_files(event, compare)
    print(f"relay as {login}: {len(files)} request file(s)")
    if not files:
        print("::warning::no new request files in this push (a request runs once, when its file is added)")
    failed = 0
    for path in files:
        name = os.path.splitext(os.path.basename(path))[0]
        try:
            req = load_request(path)
            validate(req, deny)
            result = {"ok": True, "request": path, "action": req["action"], "repo": req["repo"],
                      "requested_by": req["requested_by"],
                      **run(gh, req, login, dry_run=bool(req.get("dry_run")))}
        except (RelayError, OSError, ValueError, KeyError, TypeError) as e:
            failed += 1
            result = {"ok": False, "request": path, "error": str(e)}
        text = json.dumps(result, indent=2, ensure_ascii=False)
        with open(os.path.join(RESULT_DIR, name + ".json"), "w", encoding="utf-8") as f:
            f.write(text + "\n")
        print(f"=== RELAY RESULT {path} ===\n{text}\n=== END {path} ===")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
