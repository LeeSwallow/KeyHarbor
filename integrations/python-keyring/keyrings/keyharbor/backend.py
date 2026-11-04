"""A read-only keyring backend that asks `keyharbor run` (channel 4).

`keyring.get_password(service, username)` maps to item `service`, field `username`
(an empty username means the field `value`, which a one-value secret item has).
Values are stored with the KeyHarbor CLI or app, never through this backend.
"""

from jaraco.classes import properties
from keyring.backend import KeyringBackend
from keyring.errors import KeyringError, PasswordDeleteError, PasswordSetError

from . import client

#: Above the OS keychains (macOS Keychain is 5), so a program run under
#: `keyharbor run` asks KeyHarbor first. Outside a run the backend is not viable.
PRIORITY = 9


class KeyHarborKeyring(KeyringBackend):
    @properties.classproperty
    def priority(cls):
        if not client.available():
            raise RuntimeError("not running under keyharbor run")
        return PRIORITY

    def get_password(self, service, username):
        try:
            return client.lookup(service, username or "value")
        except client.Refused as refusal:
            if "(not_found)" in str(refusal):
                return None
            raise KeyringError(str(refusal)) from None
        except (OSError, ValueError) as error:
            raise KeyringError(f"keyharbor lookup failed: {error}") from None

    def set_password(self, service, username, password):
        raise PasswordSetError("KeyHarbor values are stored with the keyharbor CLI or app")

    def delete_password(self, service, username):
        raise PasswordDeleteError("KeyHarbor values are removed with the keyharbor CLI or app")
