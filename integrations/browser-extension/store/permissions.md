# Why each permission (for store review)

| Permission | Why |
|---|---|
| `nativeMessaging` | The extension's only channel: it talks to the KeyHarbor program on the same computer (`dev.keyharbor.host`). Nothing goes to a server. |
| `cookies` | "Hand off a site" reads the cookies of the one site the person chose and gives them to their local KeyHarbor session. |
| `optional_host_permissions` (`http://*/*`, `https://*/*`) | Asked per site, at the moment the person hands that site off, so cookie access is limited to sites they chose. |
| `activeTab` | Fill and save act on the tab where the person opened the popup, nowhere else. |
| `scripting` | Puts the login into that tab's sign-in fields (and reads the fields when saving), in the top frame only. |
| `storage` | Keeps the extension's own settings. No browsing data. |
| `notifications` | Asks the person before a site requested from the KeyHarbor app is handed off. |

**Single purpose**: connect the person's browser to their local KeyHarbor vault and agent sessions.

**Remote code**: none. All scripts are bundled in the package; the source archive is attached for review (`npm ci && npm run package` rebuilds it).
