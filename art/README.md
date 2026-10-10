# Blender asset workflow

The initiator chose live Blender MCP as the standard modeling workflow on
2026-10-08. Authority: concept repo `docs/DECISIONS.md`, “Live Blender (MCP)”.

1. Connect to live Blender through the installed `mcp-for-blender` server.
   Confirm the connection by reading the scene and capturing a viewport image.
2. Inspect the model, then iterate through MCP. Check each meaningful shape or
   material change in the viewport from the directions that expose it; use
   side-by-side variants when comparing alternatives.
3. Put accepted changes into the asset's Blender Python script.
4. Rebuild from the script and compare the rebuilt result with the live study.
   Keep generated scenes and review images under the ignored `build/` directory.

If MCP is unavailable, report the connection problem and restore the live
workflow before continuing visual design. Headless builds and exports remain
useful for reproduction and technical checks.

Keep telemetry and third-party asset integrations disabled. Use existing tools;
new dependencies follow the project approval rule.

Walker source, review generation and visual language: [`walker/README.md`](walker/README.md).
