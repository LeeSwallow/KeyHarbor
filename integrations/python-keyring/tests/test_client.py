"""Tests against a fake lookup socket. Run from integrations/python-keyring:
    python3 -m unittest discover -s tests
Not part of CI: the real socket is tested by crates/cli/tests/run_lookup_cli.rs.
"""

import json
import os
import socket
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from keyrings.keyharbor import client  # noqa: E402

VALUES = {("myapp/dev/db", "PASSWORD"): "KH-SYNTH-db", ("api", "value"): "KH-SYNTH-api"}


class FakeRun:
    """Answers like `keyharbor run`: the token, then the item."""

    def __init__(self):
        self.dir = tempfile.TemporaryDirectory()
        self.path = os.path.join(self.dir.name, "s")
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(self.path)
        self.server.listen()
        self.environ = {client.SOCKET_VAR: self.path, client.TOKEN_VAR: "t" * 64}
        threading.Thread(target=self.serve, daemon=True).start()

    def serve(self):
        while True:
            try:
                conn, _ = self.server.accept()
            except OSError:
                return
            with conn:
                request = json.loads(conn.makefile().readline())
                key = (request["item"], request["field"])
                if request["token"] != "t" * 64:
                    reply = {"error": "denied (bad-token)"}
                elif key in VALUES:
                    reply = {"value": VALUES[key]}
                else:
                    reply = {"error": f"denied: {request['item']} (not_found)"}
                conn.sendall(json.dumps(reply).encode() + b"\n")

    def close(self):
        self.server.close()
        self.dir.cleanup()


class ClientTest(unittest.TestCase):
    def setUp(self):
        self.run_ = FakeRun()

    def tearDown(self):
        self.run_.close()

    def test_a_value_comes_back(self):
        self.assertEqual(
            client.lookup("myapp/dev/db", "PASSWORD", self.run_.environ), "KH-SYNTH-db"
        )

    def test_refusals_name_no_value(self):
        with self.assertRaises(client.Refused) as caught:
            client.lookup("nope", "PASSWORD", self.run_.environ)
        self.assertEqual(str(caught.exception), "denied: nope (not_found)")
        wrong = dict(self.run_.environ, **{client.TOKEN_VAR: "x"})
        with self.assertRaises(client.Refused):
            client.lookup("api", "value", wrong)

    def test_outside_a_run_it_fails(self):
        self.assertFalse(client.available({}))
        with self.assertRaises(client.NotInRun):
            client.lookup("api", "value", {})


try:
    import keyring  # noqa: F401
except ImportError:
    keyring = None


@unittest.skipIf(keyring is None, "keyring is not installed")
class BackendTest(unittest.TestCase):
    def setUp(self):
        self.run_ = FakeRun()
        self.saved = {k: os.environ.get(k) for k in self.run_.environ}
        os.environ.update(self.run_.environ)

    def tearDown(self):
        for key, value in self.saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        self.run_.close()

    def test_get_password_asks_the_run(self):
        from keyring.errors import PasswordSetError

        from keyrings.keyharbor.backend import KeyHarborKeyring

        backend = KeyHarborKeyring()
        self.assertEqual(KeyHarborKeyring.priority, 9)
        self.assertEqual(backend.get_password("myapp/dev/db", "PASSWORD"), "KH-SYNTH-db")
        self.assertEqual(backend.get_password("api", ""), "KH-SYNTH-api")
        self.assertIsNone(backend.get_password("nope", "x"))
        with self.assertRaises(PasswordSetError):
            backend.set_password("api", "value", "new")


if __name__ == "__main__":
    unittest.main()
