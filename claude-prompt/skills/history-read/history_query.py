#!/usr/bin/env python3
import argparse
import os
import shutil
import subprocess
import sys


HELPER_DESCRIPTION = "Query DogBot text history through the RLS-protected agent_read.messages view."
MAX_LIMIT = 200


def main() -> int:
    parser = argparse.ArgumentParser(description=HELPER_DESCRIPTION)
    subparsers = parser.add_subparsers(dest="command", required=True)

    search = subparsers.add_parser("search", help="Search visible history messages")
    search.add_argument("--since", help="Only messages at or after this timestamp")
    search.add_argument("--until", help="Only messages before this timestamp")
    search.add_argument("--sender", help="actor_id exact match or actor_display substring")
    search.add_argument("--contains", help="case-insensitive substring in plain_text")
    search.add_argument("--platform-account", help="filter platform_account")
    search.add_argument("--conversation", help="filter conversation_id")
    search.add_argument("--limit", type=int, default=20, help="row limit, max 200")

    sql = subparsers.add_parser("sql", help="Run a read query against agent_read.messages")
    sql.add_argument("query", help="SQL query to execute")

    args = parser.parse_args()
    try:
        database_url, env = connection_env()
        ensure_psql()
        if args.command == "search":
            query = build_search_query(args)
        else:
            query = args.query
        return run_psql(database_url, env, query)
    except RuntimeError as err:
        print(f"history_query.py: {err}", file=sys.stderr)
        return 2


def connection_env() -> tuple[str, dict[str, str]]:
    database_url = os.environ.get("DOGBOT_HISTORY_DATABASE_URL", "").strip()
    token = os.environ.get("DOGBOT_HISTORY_RUN_TOKEN", "").strip()
    if not database_url:
        raise RuntimeError("DOGBOT_HISTORY_DATABASE_URL is not set")
    if not token:
        raise RuntimeError("DOGBOT_HISTORY_RUN_TOKEN is not set")

    env = os.environ.copy()
    env["PGOPTIONS"] = f"-c dogbot.run_token={token} -c statement_timeout=5000"
    return database_url, env


def ensure_psql() -> None:
    if shutil.which("psql") is None:
        raise RuntimeError("psql was not found in PATH")


def build_search_query(args: argparse.Namespace) -> str:
    limit = max(1, min(args.limit, MAX_LIMIT))
    where = ["true"]

    if args.since:
        where.append(f"created_at >= {sql_literal(args.since)}::timestamptz")
    if args.until:
        where.append(f"created_at < {sql_literal(args.until)}::timestamptz")
    if args.sender:
        sender = sql_literal(args.sender)
        where.append(
            f"(actor_id = {sender} OR coalesce(actor_display, '') ILIKE '%' || {sender} || '%')"
        )
    if args.contains:
        where.append(f"strpos(lower(plain_text), lower({sql_literal(args.contains)})) > 0")
    if args.platform_account:
        where.append(f"platform_account = {sql_literal(args.platform_account)}")
    if args.conversation:
        where.append(f"conversation_id = {sql_literal(args.conversation)}")

    return f"""
SELECT
    created_at,
    platform,
    platform_account,
    conversation_id,
    chat_type,
    actor_id,
    coalesce(actor_display, '') AS actor_display,
    message_id,
    plain_text
FROM agent_read.messages
WHERE {' AND '.join(where)}
ORDER BY created_at DESC, id DESC
LIMIT {limit};
""".strip()


def sql_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def run_psql(database_url: str, env: dict[str, str], query: str) -> int:
    result = subprocess.run(
        [
            "psql",
            database_url,
            "-X",
            "-v",
            "ON_ERROR_STOP=1",
            "-P",
            "pager=off",
            "--csv",
            "-c",
            query,
        ],
        env=env,
        text=True,
    )
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
