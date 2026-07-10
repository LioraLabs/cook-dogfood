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
- [core-ergonomics] `contracts/Cookfile`'s `ingredients "menu-api.yaml" "../scripts/gen_client.py"` — a `..`-relative ingredient reaching *above* the Cookfile's own directory but still inside the workspace root (`.cookroot` at repo root) — is (a) **fully supported, no caveat**: it parsed and registered with no diagnostic, and cache invalidation correctly tracked the out-of-subtree file's content. Evidence: after `cook gen` was green and fully cached (`2/2 cached`), appending `# probe\n` to `../scripts/gen_client.py` (i.e. `contracts/../scripts/gen_client.py`) and re-running `cook gen` from `contracts/` produced `gen/menuClient.ts   0.00s` / `gen   done  (1/2 cached)` — a real re-run driven solely by the script edit (`menu-api.yaml` was untouched). Re-running again with no further edits produced `2/2 cached` (new content hash memoized). `git checkout scripts/gen_client.py` to revert, then `cook gen` again, produced `2/2 cached` — the content-addressed cache hit the *original* key again, exactly as with the in-directory `menu-api.yaml` ingredient tested moments earlier. No tmpdir repro was needed since the in-repo case already demonstrates full, correct behavior; this is a clean example worth citing as "ingredients are workspace-scoped, not directory-scoped" in ergonomics docs, since it's not obvious from the surface syntax that `..` is allowed at all.
- [module:python] Toolchain probe boilerplate mirrors the Rust case from Task 3: `probe py:tools { tools { python3 } }` + `seal py:tools` is hand-rolled per Cookfile here; a `cook_python` module should ship this pre-declared (`python:tools` or similar) so consumers just `seal python:tools`. More importantly, the probe as specified only content-hashes the `python3` binary found on `PATH` — it says nothing about which packages are importable in that interpreter. This recipe's codegen script does `import yaml` (PyYAML), and PyYAML is *not* an ingredient, not probed, and not sealed: it's an ambient, undeclared determinant of `build/menuClient.ts`'s content. If PyYAML's parsing/dumping behavior changed between versions (e.g. a `safe_load` edge-case fix, or a change in dict key ordering before that was Python-version-guaranteed), cook would produce a different output with an unchanged cache key and call it a hit — a real "the build is a lie" risk for any offline codegen recipe that imports third-party libraries. A `cook_python` module should own "codegen environment" identity end-to-end: not just `which python3` but a locked/venv'd interpreter + resolved dependency set (e.g. hash a `requirements.txt`/lockfile, or run inside a probed venv) so the full determinant set is captured, the same way `cook_cc`'s probes cover compiler + flags rather than just "some cc exists on PATH".
- [module:generic] `contracts/Cookfile` is the second instance in this repo (after `tools/menugen`) of the pattern "static contract file (OpenAPI/YAML/JSON schema) → offline codegen script → generated source file consumed by another app". This is common enough (protobuf, OpenAPI, GraphQL SDL, JSON Schema-to-types) that it reads as a packageable target-maker: something like `codegen.from_spec(spec, generator_cmd, out)` that owns the ingredients-declaration/seal/mkdir-out boilerplate shown here, leaving the Cookfile author only the spec path, the generator invocation, and the output path. Right now this 4-line recipe is small enough that the boilerplate isn't painful, but it is exactly the shape that recurs.

## Verification evidence

### Task 5 — contracts/Cookfile

```
$ cd contracts && cook gen
  gen                      queued  (2 nodes)
  gen/$probe:py:tools                         0.00s
  gen/menuClient.ts                           0.00s
  gen                      done     (2/2)                    0.03s
cook done in 0.03s (2 nodes, 0 cached recipes, 1 done)
# build/menuClient.ts diffed clean against `python3 ../scripts/gen_client.py menu-api.yaml /tmp/expected.ts`

$ cook gen
  gen                      queued  (2 nodes)
  gen/$probe:py:tools                         cached
  gen/menuClient.ts                           cached
  gen                      done     (2/2 cached)             0.00s

$ printf '\n' >> menu-api.yaml && cook gen
  gen                      queued  (2 nodes)
  gen/$probe:py:tools                         cached
  gen/menuClient.ts                           0.00s
  gen                      done     (1/2 cached)             0.03s

$ git checkout menu-api.yaml && cook gen
  gen                      queued  (2 nodes)
  gen/$probe:py:tools                         cached
  gen/menuClient.ts                           cached
  gen                      done     (2/2 cached)             0.00s
# content-addressed cache hit the original key after revert, as expected

$ printf '# probe\n' >> ../scripts/gen_client.py && cook gen
  gen                      queued  (2 nodes)
  gen/$probe:py:tools                         cached
  gen/menuClient.ts                           0.00s
  gen                      done     (1/2 cached)             0.03s
# the ../scripts/gen_client.py edit alone triggered the re-run -- confirms (a): the ".." ingredient
# is a real, correctly-tracked cache determinant, not silently ignored

$ cook gen   # no further edits
  gen                      done     (2/2 cached)             0.00s

$ git checkout ../scripts/gen_client.py && cook gen
  gen                      done     (2/2 cached)             0.00s
# reverted; content-addressed cache hit the original key again
```
