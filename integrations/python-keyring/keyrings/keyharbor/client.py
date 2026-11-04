"""Asks the lookup socket of `keyharbor run` for one value (channel 4).

Standard library only. The socket and token come from the environment `keyharbor run`
gives its child; outside a run they are missing and every call fails.
"""

import json
import os
import socket

SOCKET_VAR = "KEYHARBOR_LOOKUP_SOCKET"
TOKEN_VAR = "KEYHARBOR_LOOKUP_TOKEN"
TIMEOUT_SECONDS = 5


class NotInRun(RuntimeError):
    """The program is not running under `keyharbor run`."""


class Refused(RuntimeError):
    """KeyHarbor refused the request. The message names the item and the reason only."""


def available(environ=os.environ):
    """Whether this process runs under `keyharbor run` (both variables are set)."""
    return bool(environ.get(SOCKET_VAR)) and bool(environ.get(TOKEN_VAR))


def lookup(item, field, environ=os.environ):
    """The value of `field` in `item`, e.g. lookup("myapp/dev/db", "PASSWORD").

    Raises NotInRun outside a run and Refused when KeyHarbor says no.
    """
    if not available(environ):
        raise NotInRun("not running under keyharbor run")
    request = {"token": environ[TOKEN_VAR], "item": item, "field": field}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.settimeout(TIMEOUT_SECONDS)
        conn.connect(environ[SOCKET_VAR])
        conn.sendall(json.dumps(request).encode() + b"\n")
        reply = b""
        while not reply.endswith(b"\n"):
            chunk = conn.recv(4096)
            if not chunk:
                break
            reply += chunk
    answer = json.loads(reply)
    if "value" in answer:
        return answer["value"]
    raise Refused(answer.get("error", "refused"))
