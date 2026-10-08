# art/ — models and Blender work

Rules for agents working on models. The repo-wide `AGENTS.md` applies too.

## Source

A model's only source is its Python script here; Blender runs it headless and exports the `.glb` into `content/<topic>/`. What you try live in Blender ends up in the script, and the headless run must rebuild it. Commit scripts and `.glb` files; renders and `.blend` files stay outside the repo.

Layout: one folder per topic (`walker/`, `props/`, `city/`), a topic's shared helpers in a module next to its scripts (`city/kit.py`), a topic's look and measures in its brief (`city/BRIEF.md`).

## Live Blender over the MCP

A session started in `art/` gets the `blender` MCP server (`art/.mcp.json`, `mcp-for-blender`, telemetry off). It talks to a running Blender whose add-on "MCP for Blender" listens on port 9876 (N panel in the viewport). A second agent with its own Blender uses another port.

Run a city script in the open Blender (builds into the collection `exo`, writes nothing, prints the check report):

```python
import runpy, sys; sys.modules.pop("kit", None); runpy.run_path("<repo>/art/city/shops.py", run_name="__main__")
```

Then take a viewport screenshot to look at it. The add-on's asset integrations (Poly Haven, Hyper3D, Hunyuan, Tripo, Sketchfab, Poly Pizza) stay switched off: assets are original or CC0, and generated outputs carry foreign licences.

## Done

A model is done when the headless run prints no `PROBLEM` line (or the brief records why one stays) and you have looked at its review renders:

```sh
blender -b -P art/city/shops.py -- --out content/city --renders <dir outside the repo>
```

Each render set shows the model three-quarter, from the street at eye height, from the top, and at night. Hand the render paths to the initiator; the look is theirs to decide.
