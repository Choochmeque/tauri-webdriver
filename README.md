# `tauri-driver` _(pre-alpha)_

Cross-platform WebDriver server for Tauri applications.

This is a [WebDriver Intermediary Node] that wraps the native WebDriver server
for platforms that [Tauri] supports. Your WebDriver client will connect to the
running `tauri-driver` server, and `tauri-driver` will handle starting the
native WebDriver server for you behind the scenes. It requires two separate
ports to be used since two distinct [WebDriver Remote Ends] run.

You can configure the ports used with arguments when starting the binary:

- `--port` (default: `4444`)
- `--native-port` (default: `4445`)
- `--native-host` (default: `127.0.0.1`)
- `--native-driver` (optional on Linux/Windows, required on macOS)

Supported platforms:

- **Linux** via `WebKitWebDriver`
- **Windows** via [Microsoft Edge Driver]
- **macOS** via [WebDriverAgentMac] (from [Appium Mac2 Driver])

## Installation

You can install tauri-driver using Cargo:

```sh
cargo install tauri-driver --locked
```

## macOS Setup

macOS support uses [WebDriverAgentMac], an XCUITest-based WebDriver server that
automates native macOS UI via accessibility APIs.

### Prerequisites

1. macOS 11+, Xcode 13+
2. Clone the WebDriverAgentMac project:
   ```sh
   git clone https://github.com/appium/appium-mac2-driver.git
   ```
3. Grant Accessibility permission to Xcode Helper
   (System Settings > Privacy & Security > Accessibility)

### Usage

Point `--native-driver` to the WebDriverAgentMac project directory:

```sh
tauri-driver --native-driver /path/to/appium-mac2-driver/WebDriverAgentMac
```

`tauri-driver` will run `xcodebuild` to build and launch the WebDriverAgent
test runner, then proxy WebDriver requests between your test client and WDA.

### Capabilities

Use `tauri:options` in your WebDriver capabilities, same as on other platforms:

```json
{
  "capabilities": {
    "alwaysMatch": {
      "tauri:options": {
        "application": "/path/to/YourApp.app/Contents/MacOS/YourApp"
      }
    }
  }
}
```

`tauri-driver` will automatically:

- Resolve the `.app` bundle path from the binary path
- Read the `CFBundleIdentifier` from `Info.plist`
- Translate `tauri:options` to WDA-native capabilities (`bundleId`,
  `arguments`, `environment`)
- Set `TAURI_WEBVIEW_AUTOMATION=true` and `TAURI_AUTOMATION=true` environment
  variables for the launched app
- Translate Appium-style locator strategies (`-ios predicate string` to
  `predicate string`, `-ios class chain` to `class chain`)
- Stub W3C WebDriver endpoints that WDA doesn't implement (window handles,
  title, url)

## Trying it out

Check out the documentation at https://tauri.app/develop/tests/webdriver/,
including a small example application with WebDriver tests.

[WebDriver Intermediary Node]: https://www.w3.org/TR/webdriver/#dfn-intermediary-nodes
[WebDriver Remote Ends]: https://www.w3.org/TR/webdriver/#dfn-remote-ends
[Microsoft Edge Driver]: https://developer.microsoft.com/en-us/microsoft-edge/tools/webdriver/
[WebDriverAgentMac]: https://github.com/appium/appium-mac2-driver/tree/master/WebDriverAgentMac
[Appium Mac2 Driver]: https://github.com/appium/appium-mac2-driver
[Tauri]: https://github.com/tauri-apps/tauri
