# `tauri-webdriver`

[![Crates.io](https://img.shields.io/crates/v/tauri-webdriver.svg)](https://crates.io/crates/tauri-webdriver)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Cross-platform WebDriver server for Tauri applications.

This is a [WebDriver Intermediary Node] that works with [tauri-plugin-webdriver]
to provide WebDriver automation for [Tauri] apps on macOS, Windows, and Linux.
Your WebDriver client connects to `tauri-webdriver`, which launches your Tauri
app and proxies requests to the embedded plugin. It requires two separate ports
since two distinct [WebDriver Remote Ends] run.

## Supported Platforms

| Platform | WebDriver Backend |
|----------|-------------------|
| **macOS** | [tauri-plugin-webdriver] (embedded in app) |
| **Windows** | [tauri-plugin-webdriver] (embedded in app) |
| **Linux** | [tauri-plugin-webdriver] (embedded in app) |

## Installation

```sh
cargo install tauri-webdriver --locked
```

## Command Line Options

- `--port` (default: `4444`) - Port for tauri-webdriver to listen on
- `--native-port` (default: `4445`) - Port of the plugin WebDriver
- `--native-host` (default: `127.0.0.1`) - Host of the plugin WebDriver

## Setup

On all platforms, `tauri-webdriver` works with [tauri-plugin-webdriver], which
embeds a W3C WebDriver server directly inside your Tauri application. This provides
native WebView control (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux)
without external dependencies.

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

Start `tauri-webdriver`:

```sh
tauri-webdriver
```

Configure your WebDriver client to connect to `localhost:4444` with
`tauri:options` pointing to your app binary:

```json
// macOS
{
  "capabilities": {
    "alwaysMatch": {
      "tauri:options": {
        "application": "/path/to/YourApp.app/Contents/MacOS/YourApp"
      }
    }
  }
}

// Windows
{
  "capabilities": {
    "alwaysMatch": {
      "tauri:options": {
        "application": "C:\\path\\to\\YourApp.exe"
      }
    }
  }
}

// Linux
{
  "capabilities": {
    "alwaysMatch": {
      "tauri:options": {
        "application": "/path/to/your-app"
      }
    }
  }
}
```

When a session is created, `tauri-webdriver` will:
1. Launch your Tauri app with WebDriver automation enabled
2. Wait for the plugin's HTTP server to be ready
3. Proxy all WebDriver requests to the plugin
4. Terminate the app when the session is deleted

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
[tauri-plugin-webdriver]: https://github.com/Choochmeque/tauri-plugin-webdriver
[Tauri]: https://github.com/tauri-apps/tauri
[tauri-webdriver]: https://github.com/Choochmeque/tauri-webdriver
