# Upstream relay

This branch holds no SolidCraft code. It lets our agents open pull requests, issues and comments
on the upstream projects from environments that cannot call the GitHub API for those repositories
themselves. An agent pushes a request file to the `nown-relay` branch; the workflow runs it as the
account that owns the token (NOWN-DEV), exactly as that account could by hand on github.com.

Targets: `tangency-labs/solidcraft` and `tangency-labs/solidcore` only.

## What it can do

| Action | Fields | Notes |
|---|---|---|
| `pr_create` | `title`, `head` (`NOWN-DEV:<branch>`), `base`, `body`, `draft` | Draft unless `draft: false`. Pushes to the branch update the PR directly. |
| `issue_create` | `title`, `body` | |
| `comment` | `number`, `body` | Conversation comment on an issue or PR. |
| `review_reply` | `number`, `comment_id`, `body` | Reply in a review thread. |
| `edit` | `number`, `title` and/or `body` | Own issues and PRs only. |
| `edit_comment`, `delete_comment` | `comment_id`, `comment_kind` (`issue` or `review`), `body` | Own comments only. |
| `close`, `reopen` | `number` | Own issues and PRs only. |
| `read` | `number` | Item, comments; for a PR also reviews, review comments, head SHA, mergeability, checks. |
| `list` | `state` (`open`, `closed`, `all`) | Issues and PRs this account opened. |
| `search` | `query` | GitHub issue search, limited to the target repository. |

It cannot merge, label, assign, request reviewers or close other people's items: the account has
no maintainer rights upstream.

Every request also needs `repo` and `requested_by` (who asked for it, and from which session), and
may set `dry_run: true` to see the call without making it. A long body can live in a file next to
the request: `"body_file": "<name>.md"` (inside `requests/`).

```json
{
  "action": "comment",
  "repo": "tangency-labs/solidcraft",
  "number": 8511,
  "body_file": "20261030T1100Z-8511-reply.md",
  "requested_by": "<person>, session <id>"
}
```

## Use it

1. Add `requests/<UTC stamp>-<topic>.json` (and its body file) on `nown-relay`, commit as the
   NOWN-DEV noreply identity, push. One push can carry several requests; they run in name order.
   A request runs once, when its file is added; to repeat one, add a new file.
2. The `Upstream relay` run prints each result between `=== RELAY RESULT` and `=== END` lines and
   keeps them as the `relay-results` artifact (14 days).
3. Before every push, run the same privacy scan as for any upstream push. The workflow refuses a
   write whose text matches the deny pattern as a second net, not as the first.

## Setup (once, by the account owner)

1. A classic personal access token of NOWN-DEV with only the `public_repo` scope (fine-grained
   tokens cannot write to repositories the account does not own).
2. Settings → Environments → New environment `upstream`; Deployment branches: selected branch
   `nown-relay` only. Add two environment secrets: `UPSTREAM_TOKEN` (the token) and
   `RELAY_DENY_PATTERN` (a regular expression of words that must never be posted; kept in the
   private notes, never here). The workflow runs nothing without both.

## Who can use it

Whoever can push to this repository (today only the NOWN-DEV account itself). Anyone who can push
here can also change this workflow, so access to this repository is the real gate; the
environment keeps the token away from every other branch. GitHub shows NOWN-DEV as the actor of
every run, so `requested_by` is the record of who asked.

## Tests

`cd relay && python3 -m unittest test_relay`
