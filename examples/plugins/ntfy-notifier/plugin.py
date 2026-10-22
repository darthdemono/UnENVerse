#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Example unv-plugin-rpc/1 plugin: send a notification to an ntfy topic.

The protocol is one JSON object per line on stdin and stdout. The topic URL arrives in the
environment as UNV_SLOT_URL because the user bound a vault entry to the slot named "url".
The operation "probe" never touches the network, which the conformance suite relies on.
"""
import json
import os
import sys
import urllib.request

NAME = "ntfy-notifier"
done = {}  # idempotency key -> result, so a repeated execute is not a second message


def plan(params):
    return {"steps": [{"action": "post", "to": "ntfy topic"}]}


def execute(params):
    key = params.get("idempotency_key", "")
    if key in done:
        return done[key]
    if params.get("op") != "probe":
        url = os.environ["UNV_SLOT_URL"]
        message = str(params.get("params", {}).get("message", "")).encode()
        request = urllib.request.Request(url, data=message, method="POST")
        urllib.request.urlopen(request, timeout=10).close()
    done[key] = {"status": "done"}
    return done[key]


def reconcile(params):
    return {"state": "done" if params.get("idempotency_key") in done else "unknown"}


def main():
    for line in sys.stdin:
        request = json.loads(line)
        method, params = request["method"], request.get("params", {})
        try:
            if method == "handshake":
                result = {"protocol": "unv-plugin-rpc/1", "name": NAME, "version": "1.0.0"}
            elif method == "plan":
                result = plan(params)
            elif method == "execute":
                result = execute(params)
            elif method == "reconcile":
                result = reconcile(params)
            elif method == "shutdown":
                print(json.dumps({"id": request["id"], "result": {}}), flush=True)
                return
            else:
                raise ValueError("unknown method " + method)
            print(json.dumps({"id": request["id"], "result": result}), flush=True)
        except Exception as exc:  # report, never crash the host's call
            print(json.dumps({"id": request["id"], "error": {"message": str(exc)}}), flush=True)


main()
