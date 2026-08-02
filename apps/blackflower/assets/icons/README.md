# Application icons

`blackflower-icon.svg` is the authoritative scalable application-icon source.
It preserves the project mark while adding a fixed background and platform-safe
margin for window chrome, launchers, docks, taskbars, and desktop shortcuts.

Generated assets:

| Asset | Intended use |
| --- | --- |
| `png/blackflower-icon-{size}.png` | Window and Linux desktop icons from 16 to 1024 pixels |
| `blackflower.ico` | Windows executable and shortcut icon |
| `blackflower.icns` | macOS application bundle icon |

The current application does not yet construct a native window or platform
bundle. These files are ready for that integration and must not be treated as
runtime game content.
