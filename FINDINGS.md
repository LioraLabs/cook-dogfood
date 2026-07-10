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

- [module:dotnet] Fresh `Microsoft.NET.Sdk.Web` project targeting `net10.0` on this machine's SDK (10.0.109, host runtime `10.0.9`) fails `dotnet build` with `NETSDK1226: Prune Package data not found .NETCoreApp 10.0 Microsoft.AspNetCore.App` out of the box — no code involved, this is pure toolchain state. Root cause: the SDK's package-pruning feature cross-checks the installed `Microsoft.AspNetCore.App` shared-runtime version (10.0.9) against a matching `Microsoft.AspNetCore.App.Ref` targeting pack, but this machine only has that Ref pack available via NuGet cache at 10.0.0/10.0.4 (no 10.0.9 match), and no local `/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref` at all (pacman's `dotnet-targeting-pack` only ships the `Microsoft.NETCore.App.Ref` pack, not the ASP.NET one). The error message names its own escape hatch: added `<AllowMissingPrunePackageData>true</AllowMissingPrunePackageData>` to `Api/Api.csproj`'s `PropertyGroup` (not in the task-provided template) and the build/test/run all succeeded cleanly. The downstream `Api.Tests.csproj`, despite also carrying a bare `<FrameworkReference Include="Microsoft.AspNetCore.App" />`, did **not** need the same property — the flag only had to be set on the project that owns the Web SDK. Worth flagging for Task 7's Cookfile / a `cook_dotnet` module: any Cookfile invoking `dotnet build`/`dotnet test` against `net10.0` ASP.NET Core projects on a still-preview-ish SDK install may need this property (or a corrected local pack install) to be portable across machines; a `cook_dotnet` toolchain probe should probably assert/normalize the SDK+Ref-pack pairing the same way `cook_cc`/`cook_rust` probes assert compiler/toolchain identity, rather than let each Cookfile author hand-add the workaround.
- [module:dotnet] Stamp-file pattern (`services/api/Cookfile`'s `build` recipe): `dotnet build` is not itself declarative or reproducible enough to be a cook output — it implicitly restores from NuGet on every invocation that lacks a satisfied lock (undeclared network I/O plus `~/.nuget` machine-state as an unsealed determinant), and MSBuild, not cook, owns the actual product artifacts under `Api/bin` and `Api/obj` (paths, layout, and staleness rules cook has no visibility into). The recipe therefore declares `cook "build/api-build.stamp" { dotnet build ... && echo ok > $<out> } local`: cook's own output is a one-line stamp, not the DLL. This is honest about what cook can promise (`bin`/`obj` are excluded from cook's model entirely, see below) but it opens a real correctness trap a `cook_dotnet` module must own: a warm cook cache (stamp present, hash matches, reported `cached`) says nothing about whether `Api/bin`/`Api/obj` actually exist on disk — e.g. after `git clone` into a fresh checkout with a *pre-warmed* `~/.cache/cook` or shared-store hit, cook would report the `build` recipe `cached` and skip re-running `dotnet build` entirely, leaving `bin/`/`obj/` never populated, and any downstream step that assumes MSBuild's own output directory exists (not just cook's declared `$<out>` stamp) would fail or silently use stale binaries. `dotnet test` also implicitly rebuilds (see below), which happens to paper over this for the `tests` recipe specifically since `dotnet test` re-invokes MSBuild itself — but that's incidental, not a guarantee, and a bare `dotnet run`/`dotnet Api/bin/Debug/net10.0/Api.dll` consumer would not get the same rescue. A `cook_dotnet` module's `dotnet.build()` target-maker should either (a) declare `bin`/`obj` as real cook outputs it manages end-to-end (losing the "MSBuild owns it" simplicity but closing the gap), or (b) document loudly and defensively that its stamp is a *build-happened* marker only, and any consumer must re-derive the real artifact path itself rather than trust cook's cache state as a proxy for `bin/`'s existence.
- [module:dotnet] `dotnet build`'s implicit restore is an unsealed, undeclared determinant: NuGet package resolution consults `~/.nuget/packages` (machine-global cache) and the network (nuget.org or whatever feeds `NuGet.Config` points at) on every restore that isn't already fully satisfied locally, and none of that is a cook `ingredient`, `seal`, or `probe` in this Cookfile — cook has no way to invalidate the `build` stamp if the resolved package graph changes (e.g. a floating version range resolves differently, or a package is yanked/republished) without a corresponding source-file change. The `local` disposition on `build` is the right disposition call given this (an artifact whose provenance includes ambient network/machine state should not be shared fleet-wide as if it were reproducible), but it doesn't fix the invalidation gap, only contains its blast radius to the producing machine. A `cook_dotnet` module should own this properly: ship a separate `restore` recipe/target-maker that seals a lock file (`packages.lock.json`, which `dotnet restore --use-lock-file` can produce and pin) as an `ingredient`, and have `build`/`test` invoke `dotnet build/test --no-restore` against that sealed, locked restore — turning an ambient network determinant into a declared file determinant, the same discipline `cook_cc`/`cook_rust` apply to compiler toolchains via `tools` probes.
- [module:dotnet] MSBuild's own incremental build (its `obj/*.cache`/timestamp bookkeeping under `Api/obj`) is a second, independent incrementality layer sitting *underneath* cook's — cook decides whether to re-run the `dotnet build` command at all (via the ingredients/seal fingerprint), and if it does re-run, MSBuild then separately decides internally what actually needs recompiling based on its own up-to-date checks over `obj/`. This mostly composes fine in the common case (cook re-runs the command, MSBuild does a fast incremental build inside that re-run and it's still much cheaper than `-v q`'s wall time suggests for a full rebuild), but it is a second cache with its own staleness rules layered under the first, invisible to `cook why`: a MSBuild-side false-incremental (stale `obj/` cache serving an out-of-date DLL despite cook believing the command "ran fresh") is a class of bug cook's model cannot see or account for. Not exercised as an actual failure here, but worth the module owning `dotnet clean`/`--no-incremental` as an explicit escape hatch rather than trusting MSBuild's own incrementality inside an already-incremental cook step.
- [module:dotnet] `bin/`/`obj/` exclusion: this repo's root `.gitignore` already excludes `bin/` and `obj/` globally (shared with the `apps/web`-style JS build-output exclusions), and this Cookfile never declares either directory as a cook `ingredients` glob or `cook` output — correctly, since both are MSBuild-owned scratch/product directories with their own (non-content-addressed, timestamp-heavy, machine-path-embedding) staleness model that would poison cook's cache if fingerprinted directly (e.g. `obj/*.cache` files embed absolute paths and timestamps that change on every restore regardless of source content). A `cook_dotnet` module should bake this exclusion in as a documented convention (never glob `bin`/`obj` into `ingredients`, never declare either as a `cook` output) rather than leave each Cookfile author to independently discover it.
- [module:generic] `MENU_JSON` env-var fixture-injection idiom (`test { MENU_JSON="$(realpath $<menugen.menu>)" dotnet test ... }`): worked exactly as designed, first try, no core-bug. `$<menugen.menu>` — a cross-Cookfile qualified dependency-output placeholder reaching into an `import`ed sibling Cookfile — resolved correctly inside a `test` step body (not just `cook` step bodies, confirming fixture `13-test-steps`'s `$<app>`-in-`test{}` pattern generalizes to cross-Cookfile refs too) to the *relative* path `../../tools/menugen/build/menu.json` (relative to `services/api`, the invoking Cookfile's directory — consistent with the `./$<build>` "always relative, never auto-prefixed" finding from Task 3). Wrapping it in `$(realpath ...)` at shell-invocation time, inline in the `MENU_JSON=` assignment, converts it to an absolute path before `dotnet test` forks its own subprocess (the xunit test host, whose cwd is `Api.Tests/bin/Debug/net10.0/`, several directories away from where the relative path would resolve) — exactly the trap the task called out, and exactly the fix. This is a clean, generalizable idiom for any cross-language fixture-passing case (a build/codegen artifact produced by one language's toolchain, consumed by a test suite in another): env-var injection at the `test`/`cook` step's shell prefix, `realpath`'d if the consumer's own subprocess changes cwd. Worth a `cook_generic`/testing-module helper (`test.with_env_from_dep(NAME, VAR)`) that owns the `realpath` wrapping so callers don't have to remember it's needed.
- [core-ergonomics] Sigil import (`import menugen //tools/menugen` in `services/api/Cookfile`, invoked from `services/api/` — two directory hops off the workspace root at `/home/alex/dev/cook-dogfood/.cookroot`) worked cleanly, first try, no diagnostics, no path-resolution surprises: `menugen.menu` and `menugen.build` recipe names resolved, `$<menugen.menu>` expanded correctly, and dependency ordering (menugen's `build`+`menu` recipes ran/cached before `tests`) was correct. This is a genuine positive data point for CS-0120 directory hopping plus sigil-anchored imports composing together — running `cook` from a leaf directory two levels deep, importing a sibling subtree by workspace-root-anchored path rather than a tree-relative `../../tools/menugen` (which would be rejected — no `..` in import paths per the Standard), is exactly the ergonomic story the sigil form is meant to deliver, and it delivered with zero friction.
- [core-bug] `cook build`'s live-streamed shell output for a step is mislabeled with the *previous* node's tag when the previous node was a probe that just reported `cached` and the step immediately follows it: running `build` after touching `Api/Program.cs` (forcing a real, uncached `dotnet build` to execute) prints `build/$probe:dotnet:tools    cached` followed by every line of `dotnet build`'s own stdout prefixed `[build/probe:dotnet:tools]` — e.g. `[build/probe:dotnet:tools] Build succeeded.` — even though the probe already finished and reported `cached`, and the command actually producing that output is the `build/api-build.stamp` step, which only prints its own `0.00s`/timing line *after* all of the dotnet output. The prefix names the wrong unit for the entire duration of the real work. Reproduced reliably (3 separate real-build triggers, same mislabeling every time; `-v` doesn't change it). Not filing a full minimal Rust-level repro since the mislabeling is purely cosmetic (stdout attribution in the live log, not a cache-correctness or build-correctness issue — the actual stamp file and cache key are correct per `cook why`), but it's a real diagnosability foot-gun in any DAG with more than one probe/step per recipe: a user watching live output during a slow step (e.g. this `dotnet build`, or a long `cargo build`) would misattribute the output to the wrong node when debugging which step is hanging or producing unexpected output.
- [core-bug] `local`-disposition cache is single-slot, not a true multi-version content-addressed history: for a unit with `local` (opts out of the shared store per the Standard, §{exec.cache.sharing}), the on-disk local index (`.cook/cache/build.toml`) retains exactly one recorded input/output fingerprint per unit-id, overwritten on every rebuild — so alternating a sealed ingredient between two previously-seen contents (A → B → A) forces a real rebuild on *every* transition, never hitting a "we've built this exact input combination before" shortcut, even though the *default* (non-`local`) disposition does retain that history via the shared content-addressed store and serves an immediate hit on revert. Minimal isolated repro (no dotnet involved) in a scratch `.cookroot` dir: `recipe build\n  ingredients "in.txt"\n  cook "build/out.stamp" { mkdir -p build && echo ok > $<out> } local` — writing `in.txt`=A, building (fresh), building again (`cached`), writing `in.txt`=B, building (fresh, `.cook/cache/build.toml` now holds only B's fingerprint), reverting `in.txt` to A, building: **misses and rebuilds** even though A was cached moments earlier. The identical sequence with the `local` keyword removed hits immediately on the A→B→A revert (shared-store path retains full history). Observed for real in `services/api/build`: after the Task-7 edge-1 invalidation test (editing then reverting `Api/Program.cs`), the first `cook build` post-revert cost a full rebuild despite the original content having been cached at the very start of this task. Not necessarily a spec violation — the Standard only promises `local` is "cached in the local index," not that the local index retains full history — but it's a sharp, non-obvious edge for exactly the case `local` was chosen for here (a non-shareable dotnet build artifact): any workflow that flips between two states repeatedly (e.g. switching git branches back and forth during review, or a CI matrix that shares a local-only cache dir across a small rotation of configurations) gets zero benefit from `local` caching beyond "the immediately preceding state," which is a much weaker guarantee than the "content-addressed, so it never rebuilds the same input twice" story the rest of cook's caching model advertises. Worth either documenting this bound explicitly in the `local` disposition docs, or (better) widening the local index to retain more than one fingerprint per unit-id the same way the shared store does.
- [core-ergonomics] `cook test` invoked bare (no scope argument) from `services/api/` runs **every** test recipe reachable in the whole workspace, not just the ones declared in or reachable from `services/api/Cookfile`: it discovered and ran `tools/menugen`'s `check` recipe (a `test` step recipe, imported transitively via `menugen.menu`'s dependency edge, though `tests` itself never lists `menugen.check` as a dependency) alongside `services/api`'s own `tests` recipe, reporting them together as `test result: ok. 2 passed`. This is workspace-wide test aggregation, not recipe-scoped — `cook test tests` (positional `SCOPE` argument, documented in `cook test --help`) correctly narrows to just the one recipe (`test result: ok. 1 passed`). Also notable: the default summary line reports at *cook test-unit* granularity, not xunit-fact granularity — "1 passed" for the `tests` recipe means "the one cached test-unit (the whole `dotnet test` invocation) passed," not "1 of however-many xunit `[Fact]`s passed." The underlying xunit detail (`Passed! - Failed: 0, Passed: 2, Skipped: 0, Total: 2`) is real and captured — confirmed via `--report-json`, which embeds the full `dotnet test` stdout per test-unit — but is invisible in the default terminal summary unless a test fails, `-v` is passed, or `--report-json`/`--report-junit` is requested. Anyone wiring CI off the plain terminal summary should know "N passed" is recipe/test-unit count, not underlying-framework assertion count.
- [module:pnpm] Hand-run chain (`pnpm install` → `tsc -p .` for `client`/`ui` → `esbuild --bundle --format=esm` for `app` → `node smoke.mjs`) went green first try with zero TS/esbuild config fixes needed — the `moduleResolution: "bundler"` + type-only `import type { MenuItem } from "@plateboard/client"` in `ui/src/board.ts` resolved workspace-linked `@plateboard/client`'s `.d.ts` correctly via pnpm's symlinked `node_modules`, and `app/src/main.ts`'s `import menu from "./menu.json"` under `resolveJsonModule: true` typed cleanly against `renderBoard(items: MenuItem[])` with no cast needed (none of the anticipated gotchas materialized). Two non-blocking pieces of friction worth a `cook_pnpm` module owning: (1) `pnpm install` printed `Ignored build scripts: esbuild@0.25.12` / `Run "pnpm approve-builds" ...` — esbuild's postinstall (which normally validates/registers its platform-specific prebuilt binary) is skipped by pnpm's default lifecycle-script sandboxing, yet `esbuild --version` and the real bundle both worked fine (the optional-dependency binary was installed regardless), so this is a red herring warning in this case but a `cook_pnpm` module's toolchain probe should either pre-approve known-safe build scripts (`pnpm.approve-builds`) or explicitly verify the binary is runnable rather than trust silence; a case where the ignored script *is* load-bearing would fail identically-looking but actually be broken. (2) running the esbuild-produced `dist/bundle.js` with plain `node` emits a `MODULE_TYPELESS_PACKAGE_JSON` warning (stderr, non-fatal, exit 0) because `app/package.json` has no `"type": "module"` field while the bundle is ESM (`--format=esm`); a `cook_pnpm` `esbuild.bundle()` target-maker should either default `--format=cjs` for un-typed packages or document that ESM-format consumers need `"type": "module"` in the nearest `package.json` to avoid Node's reparse-and-warn path.

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

### Task 7 — services/api/Cookfile

```
$ cd services/api && cook build
  build                    queued  (2 nodes)
  build/$probe:dotnet:tools                     0.00s
  [build/probe:dotnet:tools]
  [build/probe:dotnet:tools] Build succeeded.
  [build/probe:dotnet:tools]     0 Warning(s)
  [build/probe:dotnet:tools]     0 Error(s)
  [build/probe:dotnet:tools]
  [build/probe:dotnet:tools] Time Elapsed 00:00:00.93
  build/api-build.stamp                         0.00s
  build                    done     (2/2)                    1.08s
cook done in 1.09s (2 nodes, 0 cached recipes, 1 done)
# see [core-bug] entry above: the dotnet stdout is mislabeled [build/probe:dotnet:tools],
# not [build/api-build.stamp] -- cosmetic only, cache key/output are correct

$ cook build
  build                    queued  (2 nodes)
  build/$probe:dotnet:tools                     cached
  build/api-build.stamp                         cached
  build                    done     (2/2 cached)             0.00s
cook done in 0.00s (2 nodes, 1 cached recipes, 1 done)

$ cook test
running tests
test check@15 [menu.toml] ... ok
test tests@14 ... ok

test result: ok. 2 passed; finished in 2.0s
# bare "cook test" is workspace-wide: "check" is tools/menugen's test recipe, "tests" is
# services/api's -- see [core-ergonomics] entry above. --report-json confirms the "tests"
# unit's captured stdout: "Passed! - Failed: 0, Passed: 2, Skipped: 0, Total: 2, Duration: 46 ms"

$ cook test
running tests
test check@15 [menu.toml] ... ok (cached)
test tests@14 ... ok (cached)

test result: ok. 2 passed (2 cached); finished in 0.1s
```

Invalidation edge 1 — `Api/Program.cs` (in both `build`'s and `tests`' ingredients):

```
$ printf '\n' >> Api/Program.cs && cook test
running tests
test check@15 [menu.toml] ... ok (cached)
test tests@14 ... ok

test result: ok. 2 passed (1 cached); finished in 3.0s
# "tests" re-ran (no "(cached)"); "check" (menugen, unaffected) stayed cached.
# a follow-up `cook build` showed build/api-build.stamp already `cached` -- confirms
# `build` itself re-ran as a transitive dependency during the `cook test` invocation,
# exactly as expected (both recipes' ingredients include Api/*.cs).

$ git checkout Api/Program.cs
```

Invalidation edge 2 — `tools/menugen/menu.toml` (upstream of `menugen.menu`, consumed by `tests` only via `$<menugen.menu>`, not listed in `tests`' own `ingredients`):

```
$ printf '# edge2-nonce-...\n' >> ../../tools/menugen/menu.toml && cook test
running tests
test check@15 [menu.toml] ... ok
test tests@14 ... ok

test result: ok. 2 passed; finished in 2.0s
# both re-ran (no "(cached)" on either). menugen's menu.json content hash was verified
# byte-identical before/after the edit (sha256 ff038745... both times -- the toolchain's
# TOML parse is insensitive to a trailing comment line). NO early cutoff observed: `tests`
# re-ran solely because its upstream dependency (menugen.menu) re-ran, even though that
# dependency's output content did not change. The cross-recipe dependency edge appears to
# key on "did the upstream unit execute", not on the upstream unit's output content hash.

$ cook test        # re-run, no further edits
running tests
test check@15 [menu.toml] ... ok (cached)
test tests@14 ... ok (cached)

test result: ok. 2 passed (2 cached); finished in 0.0s

$ git checkout tools/menugen/menu.toml && cook build && cook test   # resync to clean baseline
  build                    queued  (2 nodes)
  build/$probe:dotnet:tools                     cached
  build/api-build.stamp                         cached
  build                    done     (2/2 cached)             0.00s
cook done in 0.00s (2 nodes, 1 cached recipes, 1 done)
running tests
test check@15 [menu.toml] ... ok (cached)
test tests@14 ... ok (cached)

test result: ok. 2 passed (2 cached); finished in 0.1s
```
