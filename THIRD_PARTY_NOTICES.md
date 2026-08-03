# Third-Party Notices

DevConduit includes prompt snapshots derived from the following MIT-licensed projects:

- `Jia-Ethan/codex-keysmith`, revision `628b47581291ec0b1220a7ad930f6937a187ac48`
- `Jia-Ethan/claude-keysmith`, revision `acb9d0b7ade07415c5e50117d2a55cc50a0be64e`
- `Jia-Ethan/zcode-keysmith`, revision `2fc3d9bd15476212815a64d11ff33ede8f9a5ca2`

The Codex snapshot is stored in `examples/codex-keysmith.md`, the Claude snapshot in
`examples/claude-project-rules.md`, and the ZCode snapshot in
`examples/zcode-system-role.md`.

MIT License

Copyright (c) 2026 Jia-Ethan
Copyright (c) 2026 Ethan

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## On-demand MCP sources

DevConduit does not bundle the following MCP projects in its installers. Only
after explicit user confirmation, it may download the pinned artifact into the
local application data directory, verify the listed SHA-256 digest, and prepare
an isolated runtime. The upstream project remains governed by its own license.

- `mrexodia/ida-pro-mcp`, revision `f82e6e2517a161b77e738951c3071cd446480ba0`,
  SHA-256 `3d511bb2f1439270f56e6350f9b35d4540483beb416cb8cda3905f1880a2f741`, MIT.
- `miscusi-peek/cheatengine-mcp-bridge`, revision
  `588813f3edfd2a7e0574e73d882f3383203c6343`, SHA-256
  `fdb12a0e55643ef10a04e6598cf8aef540475fd1b3779f17fe4ef92f63159416`, MIT.
- `Wasdubya/x64dbgMCP`, release `build1.1`: `x64dbg.py` SHA-256
  `6fe64ec6ea9e5df253b94ffa0274b59fd4744fb639467305ca8835288d606f25` and
  `MCP_Plugins.zip` SHA-256
  `20d0c69d0b7f2d7f251e5479cf6728be8bb5da76d3e20c9e1feb28bfbc56ce3e`, GPL-3.0.
- `PortSwigger/mcp-server`, release `v1.3.0`: `burp-mcp-all.jar` SHA-256
  `c4011245ee7da0cb901b9c0435aba3d8458ab5b0e2078e1a87fd025ed93c7892` and
  revision `5f76126409780ecba2b766c7f7388f465c5b5f94` `mcp-proxy-all.jar`
  SHA-256 `b376b860f114f67e8301e50b06760f1edd23dd99e860c3646cbeac144ce7821a`, GPL-3.0.

See each upstream repository for its complete license text, dependency notices,
and operating requirements. Python dependencies installed on demand are also
subject to their respective package licenses.
