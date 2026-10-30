import json
import os
import re
import tempfile
import unittest

import relay

DENY = re.compile(r"\bsecretproject\b", re.IGNORECASE)
REPO = "tangency-labs/solidcraft"


class FakeGitHub:
    def __init__(self, items=None):
        self.items = items or {}
        self.calls = []

    def call(self, method, path, data=None, query=None):
        self.calls.append((method, path, data))
        if method == "GET":
            return self.items[path]
        return {"number": 1, "html_url": "https://example.invalid/1"}

    def pages(self, path, query=None):
        return self.items.get(path, [])


def req(**kw):
    base = {"repo": REPO, "requested_by": "tester, session x"}
    base.update(kw)
    return base


class Validate(unittest.TestCase):
    def test_refuses_other_repos(self):
        with self.assertRaises(relay.RelayError):
            relay.validate(req(repo="someone/else", action="list"), DENY)

    def test_refuses_unknown_action(self):
        with self.assertRaises(relay.RelayError):
            relay.validate(req(action="merge", number=1), DENY)

    def test_requires_requested_by(self):
        with self.assertRaises(relay.RelayError):
            relay.validate({"repo": REPO, "action": "list"}, DENY)

    def test_privacy_pattern_blocks_writes(self):
        with self.assertRaises(relay.RelayError):
            relay.validate(req(action="comment", number=3, body="about SecretProject"), DENY)

    def test_privacy_pattern_does_not_block_reads(self):
        relay.validate(req(action="search", query="secretproject"), DENY)

    def test_missing_fields(self):
        with self.assertRaises(relay.RelayError):
            relay.validate(req(action="comment", number=3), DENY)
        with self.assertRaises(relay.RelayError):
            relay.validate(req(action="edit", number=3), DENY)

    def test_number_must_be_positive_int(self):
        for bad in (0, -1, "5", True, 1.5):
            with self.assertRaises(relay.RelayError):
                relay.validate(req(action="read", number=bad), DENY)

    def test_body_size_cap(self):
        with self.assertRaises(relay.RelayError):
            relay.validate(req(action="comment", number=1, body="x" * (relay.MAX_BODY + 1)), DENY)


class Run(unittest.TestCase):
    def test_close_only_own(self):
        gh = FakeGitHub({f"/repos/{REPO}/issues/7": {"user": {"login": "someone"}}})
        with self.assertRaises(relay.RelayError):
            relay.run(gh, req(action="close", number=7), "NOWN-DEV")
        self.assertEqual([c[0] for c in gh.calls], ["GET"])

    def test_close_own_pr_uses_pulls(self):
        gh = FakeGitHub({f"/repos/{REPO}/issues/7": {"user": {"login": "nown-dev"}, "pull_request": {}}})
        relay.run(gh, req(action="close", number=7), "NOWN-DEV")
        self.assertEqual(gh.calls[-1], ("PATCH", f"/repos/{REPO}/pulls/7", {"state": "closed"}))

    def test_edit_comment_only_own(self):
        gh = FakeGitHub({f"/repos/{REPO}/issues/comments/9": {"user": {"login": "maintainer"}}})
        with self.assertRaises(relay.RelayError):
            relay.run(gh, req(action="edit_comment", comment_id=9, body="x"), "NOWN-DEV")

    def test_delete_review_comment_own(self):
        gh = FakeGitHub({f"/repos/{REPO}/pulls/comments/9": {"user": {"login": "NOWN-DEV"}}})
        relay.run(gh, req(action="delete_comment", comment_id=9, comment_kind="review"), "NOWN-DEV")
        self.assertEqual(gh.calls[-1], ("DELETE", f"/repos/{REPO}/pulls/comments/9", None))

    def test_pr_head_must_be_our_fork(self):
        gh = FakeGitHub()
        with self.assertRaises(relay.RelayError):
            relay.run(gh, req(action="pr_create", title="t", head="other:branch", base="main"), "NOWN-DEV")
        with self.assertRaises(relay.RelayError):
            relay.run(gh, req(action="pr_create", title="t", head="branch", base="main"), "NOWN-DEV")
        self.assertEqual(gh.calls, [])

    def test_pr_create_defaults_to_draft(self):
        gh = FakeGitHub()
        relay.run(gh, req(action="pr_create", title="t", head="NOWN-DEV:fix", base="main", body="b"), "NOWN-DEV")
        method, path, data = gh.calls[-1]
        self.assertEqual((method, path, data["draft"], data["head"]), ("POST", f"/repos/{REPO}/pulls", True, "NOWN-DEV:fix"))

    def test_dry_run_writes_nothing(self):
        gh = FakeGitHub()
        out = relay.run(gh, req(action="comment", number=4, body="hi"), "NOWN-DEV", dry_run=True)
        self.assertTrue(out["dry_run"])
        self.assertEqual(gh.calls, [])

    def test_read_pr(self):
        gh = FakeGitHub({
            f"/repos/{REPO}/issues/5": {"number": 5, "title": "t", "user": {"login": "NOWN-DEV"}, "pull_request": {}},
            f"/repos/{REPO}/pulls/5": {"head": {"sha": "abc", "label": "NOWN-DEV:fix"}, "base": {"ref": "main"}},
            f"/repos/{REPO}/issues/5/comments": [{"id": 1, "body": "c", "user": {"login": "m"}}],
            f"/repos/{REPO}/commits/abc/check-runs": [{"name": "ci", "conclusion": "success"}],
        })
        out = relay.run(gh, req(action="read", number=5), "NOWN-DEV")
        self.assertEqual(out["pull"]["head_sha"], "abc")
        self.assertEqual(out["comments"][0]["author"], "m")
        self.assertEqual(out["checks"][0]["conclusion"], "success")


class Files(unittest.TestCase):
    def test_added_request_files(self):
        event = {"commits": [{"added": ["requests/b.json", "README.md", "requests/a.json"]},
                             {"added": ["requests/body.md"], "modified": ["requests/c.json"]}]}
        self.assertEqual(relay.added_request_files(event), ["requests/a.json", "requests/b.json"])

    def test_body_file_stays_in_requests(self):
        with tempfile.TemporaryDirectory() as root:
            os.makedirs(os.path.join(root, "requests"))
            with open(os.path.join(root, "secret.txt"), "w") as f:
                f.write("x")
            with open(os.path.join(root, "requests", "r.json"), "w") as f:
                json.dump({"body_file": "../secret.txt"}, f)
            with self.assertRaises(relay.RelayError):
                relay.load_request("requests/r.json", root)

    def test_body_file_read(self):
        with tempfile.TemporaryDirectory() as root:
            os.makedirs(os.path.join(root, "requests"))
            with open(os.path.join(root, "requests", "r.md"), "w") as f:
                f.write("long body")
            with open(os.path.join(root, "requests", "r.json"), "w") as f:
                json.dump({"body_file": "r.md"}, f)
            self.assertEqual(relay.load_request("requests/r.json", root)["body"], "long body")


if __name__ == "__main__":
    unittest.main()
