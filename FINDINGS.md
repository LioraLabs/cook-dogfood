# Plateboard findings — COOK-209

Tags: [core-bug] [core-ergonomics] [module:pnpm] [module:dotnet] [module:rust] [module:python] [module:generic]

Module-boundary rule: any tempting helper Lua fn / repeated shell idiom / gnarly probe body
gets recorded here as a module line item, then written inline anyway.

## Log

- [core-ergonomics] Recipe name `menu` collides with the built-in `cook menu` subcommand — `cook menu` silently ran the built-in recipe-lister (`recipe build\n  recipe menu\n  recipe check`, exit 0) instead of building anything; no error, no build/ dir, easy to mistake for a successful no-op run. The CLI does document an escape hatch (`cook +menu` invokes the colliding recipe name) and `--help` mentions it, but the natural recipe name for a `menugen` tool is exactly the word that collides. Worked around by using `cook +menu` throughout; same would apply to any recipe named `test`, `list`, `dag`, `logs`, `serve`, etc.
- [module:rust] cargo-target-dir vs cook-output copy idiom — `cook "build/menugen" { cargo build --quiet && mkdir -p build && cp target/debug/menugen $<out> } nondet` is boilerplate every Rust Cookfile will repeat: build into cargo's own `target/`, then manually `mkdir -p` + `cp` into cook's declared output path. A `cook_rust` module should ship a `rust.bin(name)` target-maker that owns the cargo invocation, target-dir plumbing, and the copy-to-`$<out>`, the same way `cook_cc` owns `cc.bin()`.
- [module:rust] toolchain probe boilerplate — `probe rust:tools\n    tools { cargo, rustc }` plus `seal rust:tools` on every recipe that compiles is hand-written per Cookfile. A `cook_rust` module should ship this probe pre-declared (module-namespaced, e.g. `rust:tools`) so consuming Cookfiles just `seal rust:tools` without redeclaring the producer.
- [core-ergonomics] `$<build>` (dep-output placeholder) expands to the bare declared output path relative to the Cookfile's cwd, e.g. `build/menugen`, not an absolute path and not already `./`-prefixed. Since cwd is not on `$PATH`, invoking it as a command required the explicit `./$<build>` spelling shown in the task description — worked as-is, no adjustment needed, but the "does it need `./`" question the task flagged is answered: yes, always, for direct execution.
- [core-ergonomics] Invalidation edge confirmed correct: appending a blank line to `menu.toml` and re-running `cook +menu` re-ran only the `menu` recipe (`menu/menu.json` recomputed) while `build` (the cargo compile + tools probe) stayed fully cached — exactly the expected dependency-scoped invalidation, no over-invalidation observed.

## Verification evidence
