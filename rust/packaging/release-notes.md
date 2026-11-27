Mindloom 2.0 is a complete rewrite in Rust. It is one native app, about a third of the size of 1.x, starts instantly and stays smooth on maps with thousands of topics. Everything from 1.1 is still here, and your maps open unchanged.

## Download

| Platform | File |
| --- | --- |
| macOS 11 or later (Apple Silicon and Intel) | `Mindloom-2.0.0-mac-universal.dmg` |
| Windows 10/11 (64-bit), installer | `Mindloom-2.0.0-win-x64.exe` |
| Windows 10/11 (64-bit), no install | `Mindloom-2.0.0-win-x64-portable.zip` |

The installers are not signed with a paid Apple or Microsoft certificate, so each system warns the first time:

- **macOS:** open the `.dmg`, drag Mindloom to Applications, then open it. If macOS blocks it, go to **System Settings → Privacy & Security** and click **Open Anyway**, or run `xattr -cr /Applications/Mindloom.app` once in Terminal.
- **Windows:** when SmartScreen appears, click **More info → Run anyway**.

## What's new

- **Rewritten in Rust**: no browser engine inside. Downloads are 12–30 MB instead of about 100 MB, and a 1,450-topic map lays out in a few milliseconds.
- **Same maps, same look**: the new engine was checked against 15,197 recordings of the 1.x engine, so layouts, colours and exports match 1.1 exactly. `.mindloom` files work in both versions.
- **Colour emoji markers** drawn from your system's emoji font.
- **Opens from Finder and Explorer**: double-click a `.mindloom` file, or use *Open With* for XMind, FreeMind and OPML files.
- **AI keys in the system keychain** (macOS Keychain, Windows Credential Manager). Connects to the provider's address with your system's own secure connection settings.
- **Windows portable build** that runs without installing.

## Everything from 1.1

Connected dragging with snap-back, re-parenting and side switching; 9 structures, 16 themes, 11 shapes, 5 line styles and 8 backgrounds; markers, tasks, notes, links, images, labels, relationships and boundaries; XMind, FreeMind, OPML, Markdown and text import; PNG, SVG, PDF and text export; templates, focus mode, presentation mode, crash-safe autosave, and the bring-your-own-key AI assistant for Claude, OpenAI, Gemini, Mistral, Groq, DeepSeek, xAI, OpenRouter, Together, Ollama, LM Studio or any compatible API.

## Known differences from 1.1

- Menus are inside the window rather than in the macOS menu bar.
- *New window* starts a second copy of the app.
