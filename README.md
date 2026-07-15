# cook-dogfood — Plateboard

A small **polyglot monorepo** built entirely with [cook](https://github.com/LioraLabs/cook).
It exists to prove one point: a single dependency graph and a single cache can
span four language stacks at once.

"Plateboard" is a toy restaurant-menu app whose pieces are deliberately written
in different languages, the way a real company's stack drifts apart over time:

- a **Rust** CLI (`tools/menugen`) that compiles `menu.toml` → `menu.json`
- a **.NET** ASP.NET Core API (`services/api`) that serves the menu, with xUnit tests
- a **TypeScript / pnpm** web workspace (`apps/web`) — three packages bundled with esbuild
- a **Python** codegen step (`contracts`) that turns an OpenAPI spec into the web
  app's typed client

There is no build script gluing these together. Each subproject has its own
`Cookfile`; the root `Cookfile` `import`s all four into one graph. `cook build`
builds the whole stack and rebuilds only the parts whose inputs actually
changed — across every language.

```console
$ cook build          # Rust + .NET + TypeScript + generated contract, one graph
$ cook build          # again — reuses the cached compiles, finishes in a fraction of a second
$ cook test           # runs the Rust, .NET, and Node tests; only what changed re-runs
```

`cook build` writes `build/manifest.txt`, a receipt of what the unified build saw:

```
Plateboard — polyglot build unified by cook
dotnet:  10.0.109
node:    v22.23.1
pnpm:    10.33.0
cargo:   1.93.1
python:  3.14.6
menu:    tools/menugen/build/menu.json
bundle:  apps/web/app/dist/bundle.js
```

## What it demonstrates

- **One graph across four languages.** The root Cookfile imports each subproject
  and depends on their outputs; cook schedules Rust, .NET, TypeScript, and Python
  work in one parallel run with one cache.
- **Cross-language data flow, tracked.** The Rust tool's `menu.json` is consumed
  by *both* the .NET API's tests (via `MENU_JSON`) and the web app
  (`app/src/menu.json`). The Python step turns `contracts/menu-api.yaml` (OpenAPI)
  into `menuClient.ts`, which is copied into the web client package. Edit the API
  spec and the TypeScript client regenerates and the bundle rebuilds — a
  cross-language cascade cook infers from declared inputs and outputs.
- **A probe that sees the toolchain.** The `stack:versions` probe shells out to
  `dotnet`/`node`/`pnpm`/`cargo`/`python3 --version` and folds the result into the
  build. The same probe value is read two ways in one recipe — as a shell sigil
  (`$<stack:versions.dotnet>`) and, in a Lua step, as `cook.probes.get(...)`.
- **Test only what changed.** `cook test` runs the Rust validator, the .NET xUnit
  suite, and the Node smoke test; results are content-keyed to the build outputs
  they consume, so unchanged code doesn't re-run its tests.
- **Core cook, no modules.** `cook.toml` declares no module dependencies on
  purpose — this exercises cook's core primitives (imports, probes, seals,
  `cook`/`test` steps, Lua bodies) rather than a language module.

## Getting the build to run

You need all four toolchains on your `PATH`, because the build really invokes
them:

| Stack | Needs | Notes |
|---|---|---|
| .NET | **.NET SDK 10** | `services/api` targets `net10.0` (recent — an older SDK won't restore it) |
| Node | **Node + pnpm** | `apps/web` is a pnpm workspace with a committed lockfile |
| Rust | **cargo / rustc** | `tools/menugen` is std-only, no crates |
| Python | **python3 + PyYAML** | `contracts` runs `gen_client.py`, which imports `yaml` |

Plus [cook](https://github.com/LioraLabs/cook) itself. Then:

```sh
cook build     # build everything
cook menu      # list every recipe and chore across the workspace
cook test      # run the whole test suite
cook why build # explain what did or didn't rebuild, and why
```

## Layout

```
cook-dogfood/
├── Cookfile              # root: imports the four subprojects + the stack:versions probe
├── cook.toml             # [modules] intentionally empty — core-only dogfood
├── contracts/            # OpenAPI spec (menu-api.yaml) + Python codegen (gen_client.py)
├── tools/menugen/        # Rust CLI: menu.toml → menu.json
├── services/api/         # .NET ASP.NET Core API + xUnit tests
└── apps/web/             # pnpm workspace: @plateboard/{client,ui,app}, esbuild bundle
```

## Not the tutorial

This is a worked example of cook at monorepo scale, not the place to learn the
language. Start with **[cook itself](https://github.com/LioraLabs/cook)** and its
manual; come back here to see the ideas composed across a real polyglot stack.

## License

MIT — see [LICENSE](LICENSE).
