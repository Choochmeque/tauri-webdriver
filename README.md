# `tauri-driver` _(pre-alpha)_

Cross-platform WebDriver server for Tauri applications.

> Fork of the [official tauri-driver](https://github.com/tauri-apps/tauri/tree/dev/crates/tauri-driver)
> with added macOS support via [tauri-plugin-webdriver].

This is a [WebDriver Intermediary Node] that wraps the native WebDriver server
for platforms that [Tauri] supports. Your WebDriver client will connect to the
running `tauri-driver` server, and `tauri-driver` will handle starting the
native WebDriver server for you behind the scenes. It requires two separate
ports to be used since two distinct [WebDriver Remote Ends] run.

## Supported Platforms

| Platform | WebDriver Backend |
|----------|-------------------|
| **Linux** | [WebKitWebDriver] |
| **Windows** | [Microsoft Edge Driver] |
| **macOS** | [tauri-plugin-webdriver] (embedded in app) |

## Installation

```sh
cargo install tauri-driver --locked
```

## Command Line Options

- `--port` (default: `4444`) - Port for tauri-driver to listen on
- `--native-port` (default: `4445`) - Port of the native WebDriver backend
- `--native-host` (default: `127.0.0.1`) - Host of the native WebDriver backend
- `--native-driver` (Linux/Windows only) - Path to native WebDriver binary

## macOS Setup

On macOS, `tauri-driver` works with [tauri-plugin-webdriver], which embeds a
W3C WebDriver server directly inside your Tauri application. This provides
native WKWebView control without external dependencies.

### 1. Add the Plugin to Your Tauri App

```sh
cargo add tauri-plugin-webdriver
```

```rust
// src-tauri/src/main.rs
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_webdriver::init())
        .run(tauri::generate_context!())
        .expect("error running app");
}
```

### 2. Build Your App

```sh
cargo tauri build
```

### 3. Run Tests

Start `tauri-driver`:

```sh
tauri-driver
```

Configure your WebDriver client to connect to `localhost:4444` with
`tauri:options` pointing to your app binary:

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

When a session is created, `tauri-driver` will:
1. Launch your Tauri app with WebDriver automation enabled
2. Wait for the plugin's HTTP server to be ready
3. Proxy all WebDriver requests to the plugin
4. Terminate the app when the session is deleted

## Linux/Windows Setup

On Linux and Windows, `tauri-driver` proxies requests to the platform's native
WebDriver server.

### Linux

Install WebKitWebDriver (usually included with WebKitGTK):

```sh
# Ubuntu/Debian
sudo apt install webkit2gtk-driver

# Fedora
sudo dnf install webkit2gtk3-devel
```

### Windows

Download [Microsoft Edge Driver] and ensure it's in your PATH, or specify the
path with `--native-driver`.

## WebDriverIO Example

```typescript
// wdio.conf.ts
export const config = {
  runner: 'local',
  specs: ['./test/**/*.ts'],
  capabilities: [{
    'tauri:options': {
      application: './src-tauri/target/release/bundle/macos/YourApp.app/Contents/MacOS/YourApp'
    }
  }],
  hostname: 'localhost',
  port: 4444,
  path: '/'
}
```

```typescript
// test/example.ts
describe('Tauri App', () => {
  it('should load the page', async () => {
    const title = await browser.getTitle()
    expect(title).toBe('My Tauri App')
  })

  it('should find elements', async () => {
    const button = await $('button#submit')
    await button.click()
  })
})
```

## Documentation

For more details, see the Tauri WebDriver documentation:
https://tauri.app/develop/tests/webdriver/

[WebDriver Intermediary Node]: https://www.w3.org/TR/webdriver/#dfn-intermediary-nodes
[WebDriver Remote Ends]: https://www.w3.org/TR/webdriver/#dfn-remote-ends
[Microsoft Edge Driver]: https://developer.microsoft.com/en-us/microsoft-edge/tools/webdriver/
[WebKitWebDriver]: https://webkitgtk.org/reference/webkit2gtk/stable/class.WebView.html
[tauri-plugin-webdriver]: https://github.com/Choochmeque/tauri-plugin-webdriver
[Tauri]: https://github.com/tauri-apps/tauri
