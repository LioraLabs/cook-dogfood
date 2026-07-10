# Plateboard findings — COOK-209

Tags: [core-bug] [core-ergonomics] [module:pnpm] [module:dotnet] [module:rust] [module:python] [module:generic]

Module-boundary rule: any tempting helper Lua fn / repeated shell idiom / gnarly probe body
gets recorded here as a module line item, then written inline anyway.

## Summary

| Tag | Count |
|---|---|
| [core-bug] | 3 |
| [core-ergonomics] | 17 |
| [module:pnpm] | 4 |
| [module:dotnet] | 5 |
| [module:rust] | 2 |
| [module:python] | 1 |
| [module:generic] | 6 |
| **Total** | **38** |

Of 38 tagged findings, three are [core-bug]s and warrant priority triage against the unlanded milestone. The most severe is the `test`-step vs `cook`-step dependency-fold inconsistency and its false-cache-HIT addendum: `cook`-step units fold an upstream dependency's *output content* (enabling early cutoff) while `test`-step units fold the upstream's *execution* (systematic over-invalidation) — and, worse, a bare `cook test` registers the full workspace graph and re-records local-index input fingerprints for units that never ran, so a subsequent `cook typecheck`/`cook why` can report a `HIT` on content the underlying tool never actually saw: genuine cache poisoning. The second core-bug is that the `local`-disposition cache is single-slot last-build-wins — alternating an input A→B→A always rebuilds on revert, where the default shared-store disposition hits immediately — which erases most of `local`'s caching value for branch-switching workflows. The third is cosmetic but real: live build-log output is mislabeled with the previous (already-cached) probe's node name for the full duration of the actual work. (Relatedly, fan-out members' display labels can render truncated and identical within one invocation — tracked as [core-ergonomics].) On the ergonomics side, the two heavyweights are that a `recipe A: B` header-only dependency is ordering-only and contributes nothing to the cache key unless the body references `$<B>` directly — forcing a `: $<dep> &&` no-op-reference idiom as the only known workaround — and that the built-in `cook menu` subcommand silently shadows any user recipe literally named `menu`, exiting 0 with no build output and no error. Module line items skew toward dotnet (5, mostly toolchain/restore/incrementality gaps a `cook_dotnet` module should own outright) and pnpm (4, centered on the per-package dependency-topology gap where `typecheck`'s fan-out only fingerprints the package *list*, not each member's source tree).

## Log

### [core-bug] (3)

- [core-bug] `cook build`'s live-streamed shell output for a step is mislabeled with the *previous* node's tag when the previous node was a probe that just reported `cached` and the step immediately follows it: running `build` after touching `Api/Program.cs` (forcing a real, uncached `dotnet build` to execute) prints `build/$probe:dotnet:tools    cached` followed by every line of `dotnet build`'s own stdout prefixed `[build/probe:dotnet:tools]` — e.g. `[build/probe:dotnet:tools] Build succeeded.` — even though the probe already finished and reported `cached`, and the command actually producing that output is the `build/api-build.stamp` step, which only prints its own `0.00s`/timing line *after* all of the dotnet output. The prefix names the wrong unit for the entire duration of the real work. Reproduced reliably (3 separate real-build triggers, same mislabeling every time; `-v` doesn't change it). Not filing a full minimal Rust-level repro since the mislabeling is purely cosmetic (stdout attribution in the live log, not a cache-correctness or build-correctness issue — the actual stamp file and cache key are correct per `cook why`), but it's a real diagnosability foot-gun in any DAG with more than one probe/step per recipe: a user watching live output during a slow step (e.g. this `dotnet build`, or a long `cargo build`) would misattribute the output to the wrong node when debugging which step is hanging or producing unexpected output.

- [core-bug] `local`-disposition cache is single-slot, not a true multi-version content-addressed history: for a unit with `local` (opts out of the shared store per the Standard, §{exec.cache.sharing}), the on-disk local index (`.cook/cache/build.toml`) retains exactly one recorded input/output fingerprint per unit-id, overwritten on every rebuild — so alternating a sealed ingredient between two previously-seen contents (A → B → A) forces a real rebuild on *every* transition, never hitting a "we've built this exact input combination before" shortcut, even though the *default* (non-`local`) disposition does retain that history via the shared content-addressed store and serves an immediate hit on revert. Minimal isolated repro (no dotnet involved) in a scratch `.cookroot` dir: `recipe build\n  ingredients "in.txt"\n  cook "build/out.stamp" { mkdir -p build && echo ok > $<out> } local` — writing `in.txt`=A, building (fresh), building again (`cached`), writing `in.txt`=B, building (fresh, `.cook/cache/build.toml` now holds only B's fingerprint), reverting `in.txt` to A, building: **misses and rebuilds** even though A was cached moments earlier. The identical sequence with the `local` keyword removed hits immediately on the A→B→A revert (shared-store path retains full history). Observed for real in `services/api/build`: after the Task-7 edge-1 invalidation test (editing then reverting `Api/Program.cs`), the first `cook build` post-revert cost a full rebuild despite the original content having been cached at the very start of this task. Not necessarily a spec violation — the Standard only promises `local` is "cached in the local index," not that the local index retains full history — but it's a sharp, non-obvious edge for exactly the case `local` was chosen for here (a non-shareable dotnet build artifact): any workflow that flips between two states repeatedly (e.g. switching git branches back and forth during review, or a CI matrix that shares a local-only cache dir across a small rotation of configurations) gets zero benefit from `local` caching beyond "the immediately preceding state," which is a much weaker guarantee than the "content-addressed, so it never rebuilds the same input twice" story the rest of cook's caching model advertises. Worth either documenting this bound explicitly in the `local` disposition docs, or (better) widening the local index to retain more than one fingerprint per unit-id the same way the shared store does.

- [core-bug] test-step dep folding is inconsistent with cook-step dep folding. For a `cook`-step unit, a `$<dep>` body reference folds the upstream's *output content* — early cutoff works: changing `lib`'s command so `build/lib.txt`'s bytes stay byte-identical to a prior cached run left `useref` cached (no re-run). For a `test`-step unit, the same reference folds the upstream's *execution*, not its output content. Minimal repro, same Cookfile as above plus:

      recipe tcheck: lib
          ingredients "t.txt"
          test { : $<lib>; true }

  With `lib`'s command changed so `build/lib.txt`'s content stays byte-identical: `useref` (cook-step) stays cached; `tcheck` (test-step) loses its "(cached)" status and re-runs. This means test suites systematically over-invalidate relative to cook-step siblings consuming the identical dependency — and it explains an earlier live observation in this repo (`api`'s xunit suite re-ran when `menu.toml` changed even though the generated `menu.json` was byte-identical both times, logged above as Task 7's "Invalidation edge 2"). `apps/web/Cookfile`'s `smoke` test (this rework) now references `$<bundle>` and will inherit this same over-invalidation — accepted for this rework since missing a real change is worse than an occasional unnecessary re-run, but it's a real, unrouted-around inconsistency in the caching model.

  Additionally discovered while running this rework's own verification script — a second, more concerning failure mode in the same neighborhood: a **false cache HIT**, not just over-invalidation. Reproduced 3× in `apps/web` with fresh, never-before-seen `packages/ui/src/board.ts` content each time: `cook bundle` (correctly re-runs `ui` + `bundle` on the new content) → `cook test` (correctly re-runs `smoke`) → `cook typecheck` incorrectly reports all 3 fan-out members `cached`, despite `typecheck`'s body now correctly referencing `$<client> $<ui> $<menudata>` (confirmed via `cook why typecheck`, whose reported input list *does* include the current `packages/ui/dist/board.js`/`.d.ts`) and despite this exact content never having been typechecked before. Isolated the trigger precisely: `cook typecheck` run directly after `cook bundle` (no intervening `cook test`) correctly re-runs all 3 members, every time, across 5+ independent content edits; inserting a `cook test` between the two makes the immediately-following `cook typecheck` wrongly report cached, every time, also across 5+ independent content edits. This looks like `cook test` leaving some cross-command cache/hash state that a subsequent, unrelated `cook typecheck` invocation trusts without re-verification — a correctness bug in the shared/persistent cache layer distinct from (and worse than) the execution-vs-content-fold inconsistency above, since it produces a **false green** on content `cook` has never seen before. Not root-caused past trigger isolation (`cook`'s Rust internals are out of scope for this dogfood task); flagging for engine follow-up. `git status` confirmed clean, no dangling state left in `apps/web` after reverting.

  Checkpoint-3 mechanism finding: bare `cook test` registers the full workspace recipe graph, and registration-without-execution re-records local-index input fingerprints from the current filesystem for units that never ran — verified via `.cook/cache/typecheck.toml` snapshots, where the recorded input hash advanced from old to new content after `cook test` executed zero `typecheck` units, and a subsequent `cook why typecheck` then reported `HIT (local)` on content the `tsc` invocation never actually saw. Any command that registers-without-executing (not just `cook test`) likely poisons the cache the same way.

### [core-ergonomics] (17)

- [core-ergonomics] Recipe name `menu` collides with the built-in `cook menu` subcommand — `cook menu` silently ran the built-in recipe-lister (`recipe build\n  recipe menu\n  recipe check`, exit 0) instead of building anything; no error, no build/ dir, easy to mistake for a successful no-op run. The CLI does document an escape hatch (`cook +menu` invokes the colliding recipe name) and `--help` mentions it, but the natural recipe name for a `menugen` tool is exactly the word that collides. Worked around by using `cook +menu` throughout; same would apply to any recipe named `test`, `list`, `dag`, `logs`, `serve`, etc.

- [core-ergonomics] `$<build>` (dep-output placeholder) expands to the bare declared output path relative to the Cookfile's cwd, e.g. `build/menugen`, not an absolute path and not already `./`-prefixed. Since cwd is not on `$PATH`, invoking it as a command required the explicit `./$<build>` spelling shown in the task description — worked as-is, no adjustment needed, but the "does it need `./`" question the task flagged is answered: yes, always, for direct execution.

- [core-ergonomics] Invalidation edge confirmed correct: appending a blank line to `menu.toml` and re-running `cook +menu` re-ran only the `menu` recipe (`menu/menu.json` recomputed) while `build` (the cargo compile + tools probe) stayed fully cached — exactly the expected dependency-scoped invalidation, no over-invalidation observed.

- [core-ergonomics] `contracts/Cookfile`'s `ingredients "menu-api.yaml" "../scripts/gen_client.py"` — a `..`-relative ingredient reaching *above* the Cookfile's own directory but still inside the workspace root (`.cookroot` at repo root) — is (a) **fully supported, no caveat**: it parsed and registered with no diagnostic, and cache invalidation correctly tracked the out-of-subtree file's content. Evidence: after `cook gen` was green and fully cached (`2/2 cached`), appending `# probe\n` to `../scripts/gen_client.py` (i.e. `contracts/../scripts/gen_client.py`) and re-running `cook gen` from `contracts/` produced `gen/menuClient.ts   0.00s` / `gen   done  (1/2 cached)` — a real re-run driven solely by the script edit (`menu-api.yaml` was untouched). Re-running again with no further edits produced `2/2 cached` (new content hash memoized). `git checkout scripts/gen_client.py` to revert, then `cook gen` again, produced `2/2 cached` — the content-addressed cache hit the *original* key again, exactly as with the in-directory `menu-api.yaml` ingredient tested moments earlier. No tmpdir repro was needed since the in-repo case already demonstrates full, correct behavior; this is a clean example worth citing as "ingredients are workspace-scoped, not directory-scoped" in ergonomics docs, since it's not obvious from the surface syntax that `..` is allowed at all.

- [core-ergonomics] Sigil import (`import menugen //tools/menugen` in `services/api/Cookfile`, invoked from `services/api/` — two directory hops off the workspace root at `/home/alex/dev/cook-dogfood/.cookroot`) worked cleanly, first try, no diagnostics, no path-resolution surprises: `menugen.menu` and `menugen.build` recipe names resolved, `$<menugen.menu>` expanded correctly, and dependency ordering (menugen's `build`+`menu` recipes ran/cached before `tests`) was correct. This is a genuine positive data point for CS-0120 directory hopping plus sigil-anchored imports composing together — running `cook` from a leaf directory two levels deep, importing a sibling subtree by workspace-root-anchored path rather than a tree-relative `../../tools/menugen` (which would be rejected — no `..` in import paths per the Standard), is exactly the ergonomic story the sigil form is meant to deliver, and it delivered with zero friction.

- [core-ergonomics] `cook test` invoked bare (no scope argument) from `services/api/` runs **every** test recipe reachable in the whole workspace, not just the ones declared in or reachable from `services/api/Cookfile`: it discovered and ran `tools/menugen`'s `check` recipe (a `test` step recipe, imported transitively via `menugen.menu`'s dependency edge, though `tests` itself never lists `menugen.check` as a dependency) alongside `services/api`'s own `tests` recipe, reporting them together as `test result: ok. 2 passed`. This is workspace-wide test aggregation, not recipe-scoped — `cook test tests` (positional `SCOPE` argument, documented in `cook test --help`) correctly narrows to just the one recipe (`test result: ok. 1 passed`). Also notable: the default summary line reports at *cook test-unit* granularity, not xunit-fact granularity — "1 passed" for the `tests` recipe means "the one cached test-unit (the whole `dotnet test` invocation) passed," not "1 of however-many xunit `[Fact]`s passed." The underlying xunit detail (`Passed! - Failed: 0, Passed: 2, Skipped: 0, Total: 2`) is real and captured — confirmed via `--report-json`, which embeds the full `dotnet test` stdout per test-unit — but is invisible in the default terminal summary unless a test fails, `-v` is passed, or `--report-json`/`--report-junit` is requested. Anyone wiring CI off the plain terminal summary should know "N passed" is recipe/test-unit count, not underlying-framework assertion count.

- [core-ergonomics] Cross-Cookfile placeholder expansion datapoint (`client`'s `cp $<contracts.gen> $<out>`, `apps/web/Cookfile` importing `contracts //contracts`): confirmed via the build event log (`.cook/logs/*/events.jsonl`, `node-started` event for the `client` recipe's first cook step) that `$<contracts.gen>` expands to the literal string `../../contracts/build/menuClient.ts` — i.e. relative to `apps/web` (the *consuming* Cookfile's directory, two hops from the repo root), not an absolute path, not workspace-root-relative, and not relative to `contracts/` (the *producing* Cookfile's directory). This is consistent with the Task 3/7 finding that `$<dep>` placeholders are always bare-relative-to-the-referencing-Cookfile's-cwd, generalized here to a placeholder qualified with an *imported* recipe's namespace (`contracts.gen`, sigil-imported via `//contracts`) rather than a same-Cookfile or directory-hopped-import (`menugen.menu`) dependency — no new shape, no caveat, the relative-path-computed-from-consumer-cwd rule holds uniformly across same-file deps, directory-hopped imports, and workspace-root-sigil imports alike. The `cp` worked with zero adjustment; no `realpath` was needed here (unlike the Task 7 `MENU_JSON` env-var case) because `cp`'s own cwd during the step is already `apps/web` (the Cookfile's directory) and the relative path is correct from that same cwd — the earlier case only needed `realpath` because `dotnet test` forked a subprocess with a *different* cwd than the Cookfile's.

- [core-ergonomics] Fan-out invalidation observation (edit `packages/ui/src/board.ts`, append `export const BOARD_VERSION = 1;`, run `cook bundle` then `cook typecheck`): **exactly one** unit re-ran across both invocations — `ui`'s own `dist/board.js`+`dist/board.d.ts` tsc step (correctly, since `packages/ui/src/*.ts` is a direct glob `ingredients` of the `ui` recipe). Everything downstream stayed `cached`: `bundle/bundle.js` (depends on `ui` via the recipe-level `: ui menudata deps` header but never references `$<ui>` in its body — confirmed stale by direct inspection, `app/dist/bundle.js`'s mtime predated the edit and `grep -c BOARD_VERSION app/dist/bundle.js` = 0 even though `packages/ui/dist/board.js` — the file esbuild's bundler actually resolves through `node_modules` — did contain the new symbol) and all three `typecheck` fan-out stamps (`build/typecheck/{app,packages/client,packages/ui}.stamp`, all cached with mtimes predating the edit — `pnpm --filter @plateboard/ui exec tsc -p . --noEmit` was never re-invoked against the changed `board.ts`, despite that file being the fan-out unit's own package source, because `typecheck`'s only declared `ingredients` is the `pnpm:packages` probe fingerprinting the `{name,dir}` *list*, not each member's source tree). This confirms the mechanism precisely: a recipe-level `: dep1 dep2` header is an **ordering-only** edge unless the body actually references `$<dep>` as a placeholder (compare Task 7's edge-2 finding, where `tests: build menugen.menu` DID couple because the body used `$<menugen.menu>` directly) — recipe dependency + fan-out member identity do NOT automatically fold a listed dependency's rebuilt *content* into a consuming unit's cache fingerprint. Reverting `packages/ui/src/board.ts` and re-running `cook bundle` hit the shared content-addressed store immediately (`ui/board.js cached` on the very next run, matching the earlier revert-hits-immediately finding for non-`local` recipes) confirming no state was left dangling by the stale-cache episode; git working tree confirmed clean after the revert.

- [core-ergonomics] Fan-out unit display-label truncation, cosmetic only: the live build log's node label for the `typecheck` fan-out shows just `typecheck/client.stamp`, `typecheck/ui.stamp`, `typecheck/app.stamp` — but the actual declared/produced paths are `build/typecheck/packages/client.stamp`, `build/typecheck/packages/ui.stamp`, `build/typecheck/app.stamp` (confirmed via direct `ls`; the `packages/` path segment from `$<in.dir>` is present on disk but dropped from the printed label, which instead shows only the output pattern's basename-like tail). Not a correctness issue (the files exist at the right paths, `cook why`/cache keys are unaffected) but a real diagnosability wrinkle for exactly the case fan-out is meant to make legible: a user scanning the live log to confirm "did the `packages/client` member's typecheck run" sees a label indistinguishable from a flat, non-nested member.

  Checkpoint-3 follow-up: the behavior is worse than originally recorded above. Re-observed on this rework's own `cook typecheck` runs (e.g. right after the `deps`-stamp rework invalidated all three fan-out members): the live log printed the exact same label three times in a row — `typecheck/app.stamp`, `typecheck/app.stamp`, `typecheck/app.stamp` on one run, and `typecheck/client.stamp`, `typecheck/client.stamp`, `typecheck/client.stamp` on another — not merely truncated-but-distinguishable per member, but genuinely identical across all three members within a single invocation. Labels are not a reliable way to tell which fan-out member corresponds to which log line; only `ls`/`cook why` against the actual output paths can answer "did member X run."

- [core-ergonomics] No JS-brace-vs-cook-brace-lexer issue: the `pnpm:packages` probe's `json { node -e '...' }` body contains multiple `{`/`}` pairs (object literals, arrow-less function bodies in the `.map()`/`.filter()` chain) nested inside cook's own `{ }` producer-body delimiters, and it parsed and ran correctly on the first attempt with no simplification needed — cook's brace-balance lexer for shell-body producers handles balanced-brace JS content transparently. Recorded per the task's instruction to note this either way; no `[core-bug]` filed since nothing broke.

- [core-ergonomics] Header-only recipe dep is ordering-only — confirmed via a controlled tmpdir experiment (independent of, and more rigorous than, the live `apps/web` observation logged below): a `recipe A: B` edge with no `$<B>` placeholder referenced anywhere in A's body contributes *nothing* to A's cache key, only build ordering. Minimal repro:

      recipe lib
          ingredients "src.txt"
          cook "build/lib.txt" { mkdir -p build && cp src.txt $<out> }

      recipe useref: lib
          cook "build/useref.txt" { : $<lib>; printf 'nonce=%s\n' "$(date +%s%N)" > $<out> }

      recipe noref: lib
          cook "build/noref.txt" { printf 'nonce=%s\n' "$(date +%s%N)" > $<out> }

  Editing `src.txt` re-ran `useref` (references `$<lib>`) but left `noref` (header-only dep, no reference) cached — stale. Spec-consistent: §17.1.1 promises caching "over exactly what the author declared," and §10.6 documents header edges as ordering-only ("observable through execution ordering") — but the surface reads exactly like a dependency declaration to an author skimming `recipe A: B`, and the live consequence was real here: `apps/web/Cookfile`'s original `bundle`/`ui`/`typecheck`/`smoke` recipes all had this shape, and an unreferenced `ui` edit left `app/dist/bundle.js` stale-but-reported-green (see Task 9 evidence below). Workaround applied in this rework: the `: $<dep>` no-op shell reference (POSIX `:` builtin, ignores its arguments, exists purely to force placeholder expansion) at the start of the consuming body. Whether cook v1.0 should instead fold declared header-dep edges into the cache key by default (making the ordering-only surface an explicit opt-out rather than a silent default) is a real design question for the language cut.

- [core-ergonomics] Recipe-level `ingredients` in a multi-`cook`-step recipe binds only to the step that consumes it as its iteration source (here, the first `cook` step), not to every `cook` step in the recipe body — confirmed via `cook why client` after adding `ingredients "packages/client/tsconfig.json" "packages/client/package.json"` to `client` (a two-step recipe: a `cp` step then a `tsc` step): the first step's declared `inputs:` correctly gained both config files, but the second (`tsc`) step's `inputs:` still listed only `build/pnpm-install.stamp` and the first step's output file — editing `tsconfig.json` re-ran the `cp` step (harmlessly, its body ignores the new inputs) but left the `tsc` step `cached`, because the `cp` step's output bytes were unchanged (early cutoff). The Standard's at-most-one-`ingredients`-per-recipe-body rule (§6, CS-normative) makes a second, step-scoped `ingredients` declaration impossible within one recipe — the actual fix (applied here) is `$<file:PATH>` (CS-0101, §{phl.cook-step}), a placeholder that folds an arbitrary file's content into *that specific step's* declared inputs regardless of `ingredients` scope: `: $<deps> $<file:packages/client/tsconfig.json> $<file:packages/client/package.json> && ...` in the `tsc` step's own body closed the gap, verified by `cook why` showing both files in that unit's `inputs:` and by a live edit-then-rebuild correctly re-running just that step. Worth noting for anyone extrapolating the `ingredients`+`seal` idiom to a multi-step recipe: it silently covers only one step, and `$<file:PATH>` (not `ingredients`) is the per-step escape hatch.

- [core-ergonomics] `cook why <recipe>` reports each unit's cache status by re-deriving that unit's own key from its own declared inputs against the *current* on-disk/index state — it does not cascade-predict what a subsequent `cook build` would do to units further downstream. Observed live in Task 11: after editing `tools/menugen/src/main.rs` (the direct `ingredients` of `menugen.build`), `$COOK why build` from the workspace root correctly flagged `build :: build/menugen@... [MISS (shared)]` (its `src/main.rs` input hash differs from any known key, with `shared-miss diff: no producer manifest published for this key`), but every downstream unit in the same report — `menu :: build/menu.json`, `menudata :: app/src/menu.json`, `bundle :: app/dist/bundle.js`, `build :: build/manifest.txt` — still reported `[HIT (local)]`, because their listed `inputs:` still show the *old*, still-on-disk `build/menugen` content hash (e.g. `build/menugen  3707fb34781d0e70`), not the hash that a rebuilt `menugen` binary would produce. This is internally consistent (why is describing the graph as it stands, not simulating a future run) but easy to misread as "only `menugen.build` will re-run" when actually every downstream consumer that references `$<menugen.build>`/`$<menugen.menu>` content will also miss once the direct MISS is resolved and its output content changes. Anyone using `cook why` to scope the blast radius of a pending edit before running `cook build` should read it as "confirmed direct cause only," not "full downstream impact," and re-run `cook why` on the specific downstream recipe of interest *after* the rebuild to confirm propagation, not before.

- [core-ergonomics] `client`'s `cp $<contracts.gen> $<out>` step (the first of its two `cook` steps) re-runs on an unrelated `tsconfig.json` edit because recipe-level `ingredients` binds only to the recipe's first `cook` step (see the multi-step `ingredients`-scoping finding above) — a `tsconfig.json` change re-invokes the harmless `cp`, not just the `tsc` step that actually cares. Harmless (the `cp` step's output content is unaffected, so early cutoff still spares the downstream `tsc` step), but worth flagging as one more surface of the same root cause.

- [core-ergonomics] Root aggregation (`import contracts ./contracts` / `import menugen ./tools/menugen` / `import api ./services/api` / `import web ./apps/web`, all tree-relative `./` sigils from the workspace root) worked first try, zero friction: recipe names auto-namespace by import alias (`api.build`, `web.bundle`, `web.typecheck`, `menugen.menu`, `contracts.gen`), header deps on imported recipes ordered the whole four-stack DAG correctly, and both probe-consumption modes in the same recipe body — `$<stack:versions.FIELD>` shell-sigil injection in the first `cook` step and `cook.probes.get("stack:versions")` inside the second `cook ... >{ lua }` step — resolved correctly with no adaptation needed. `fs.write(output, ...)` in the Lua step wrote `build/manifest.lua.txt` correctly (auto-creating `build/` per §{lua.fs-write}, so the `mkdir -p build` in the sibling shell step was redundant for the Lua step specifically, though still required for the shell step's own `$<out>` redirect).

- [core-ergonomics] Latent probe-scheduling gap, not triggered here but worth flagging: per §{cat.probes.exec}/§22.5's demand-driven-scheduling rule, a probe unit only executes if some *scheduled non-probe unit lists the probe's key in its `probes` field* — and per `cook-luagen/src/cook_step.rs`, a declarative `cook "path" >{ lua }` step (`Body::LuaBlock`) never gets an auto-populated `probes` field the way a shell `Body::ShellBlock` step does via `$<key.field>` sigil-scanning (`expand_command_template`); the generated `cook.add_unit` call for a Lua-body step carries no `probes = {...}` entry at all, regardless of whether the Lua source calls `cook.probes.get(...)`. In this Cookfile the second (Lua) `cook` step reading `stack:versions` shares a recipe with the first (shell) step, whose `$<stack:versions.dotnet>` etc. placeholders DO populate that step's `probes` field and thus demand-schedule the probe — so the Lua step's `cook.probes.get` call always found the value populated, ordering worked, no failure observed across the cold build, the sanity re-run, and the no-op rebuild. But a Cookfile with a probe consumed *only* from a `>{ lua }` step's `cook.probes.get(...)` call — no sibling shell step referencing the same probe by sigil — would have no `probes`/`requires` edge demanding that probe at all, and per the demand-driven-scheduling rule the probe unit "MUST NOT execute" in that case; `cook.probes.get` would then read `nil` from an unpopulated per-run store rather than erroring loudly. Recommend either (a) extending the same `$<key.field>`-sigil pre-scan cook already does for shell bodies to Lua-body text so `cook.probes.get("ns:key")` calls auto-populate `probes`, or (b) a register-time diagnostic when a `>{ lua }` body's literal `cook.probes.get("...")` argument names a probe absent from that unit's `probes` list.

- [core-ergonomics] Recipe-level `seal` appears not to fold into TEST-step unit keys: adding `seal web:tools` to the `smoke` recipe (test-only body) produced no key change — the very next `cook test` reported the suite `(cached)` with no re-run, whereas the same edit on a cook-step recipe re-keys its units. Consistent with the Standard's letter (§8.4.3 rule 1 scopes the baseline to "every `cook` unit in the recipe"), so possibly by design — but a `seal` line in a test-only recipe is then a silent no-op the author has no feedback about; a register-time note/diagnostic ("seal has no cook units to apply to") would prevent authors believing their test suite is toolchain-keyed when it isn't. (Test suites still fold their ingredients and dep executions; only the probe-seal path is inert.)

### [module:pnpm] (4)

- [module:pnpm] Hand-run chain (`pnpm install` → `tsc -p .` for `client`/`ui` → `esbuild --bundle --format=esm` for `app` → `node smoke.mjs`) went green first try with zero TS/esbuild config fixes needed — the `moduleResolution: "bundler"` + type-only `import type { MenuItem } from "@plateboard/client"` in `ui/src/board.ts` resolved workspace-linked `@plateboard/client`'s `.d.ts` correctly via pnpm's symlinked `node_modules`, and `app/src/main.ts`'s `import menu from "./menu.json"` under `resolveJsonModule: true` typed cleanly against `renderBoard(items: MenuItem[])` with no cast needed (none of the anticipated gotchas materialized). Two non-blocking pieces of friction worth a `cook_pnpm` module owning: (1) `pnpm install` printed `Ignored build scripts: esbuild@0.25.12` / `Run "pnpm approve-builds" ...` — esbuild's postinstall (which normally validates/registers its platform-specific prebuilt binary) is skipped by pnpm's default lifecycle-script sandboxing, yet `esbuild --version` and the real bundle both worked fine (the optional-dependency binary was installed regardless), so this is a red herring warning in this case but a `cook_pnpm` module's toolchain probe should either pre-approve known-safe build scripts (`pnpm.approve-builds`) or explicitly verify the binary is runnable rather than trust silence; a case where the ignored script *is* load-bearing would fail identically-looking but actually be broken. (2) running the esbuild-produced `dist/bundle.js` with plain `node` emits a `MODULE_TYPELESS_PACKAGE_JSON` warning (stderr, non-fatal, exit 0) because `app/package.json` has no `"type": "module"` field while the bundle is ESM (`--format=esm`); a `cook_pnpm` `esbuild.bundle()` target-maker should either default `--format=cjs` for un-typed packages or document that ESM-format consumers need `"type": "module"` in the nearest `package.json` to avoid Node's reparse-and-warn path.

- [module:pnpm] `pnpm.workspace_packages()` probe (written before authoring `apps/web/Cookfile`'s `pnpm:packages` probe, per the module-boundary rule): the gnarly probe body needed here is a `node -e` one-liner that reads `pnpm-workspace.yaml`'s package globs (hand-expanded to `packages/*` + `app` rather than actually parsing the YAML — a real implementation would need a YAML parser or `pnpm ls -r --json`), filters to dirs with a `package.json`, and emits `[{name, dir}, ...]` as JSON on stdout for the `json {}` probe producer to parse. This exact shape — enumerate workspace member dirs, read each `package.json`'s `name` field, emit a `{name, dir}` record array — is something every pnpm/npm/yarn-workspace Cookfile will need for per-package fan-out (typecheck, lint, per-package test, per-package publish), and hand-rolling it against `fs.readdirSync`+glob-by-hand is fragile (it silently misses nested/scoped package dirs, doesn't honor negated globs, and would need updating if `pnpm-workspace.yaml`'s `packages:` list changes shape). A `cook_pnpm` module should ship this as a pre-declared probe (e.g. `pnpm:packages`, populated via `pnpm ls -r --json --depth -1` under the hood, which pnpm already exposes) so consuming Cookfiles just write `ingredients pnpm:packages` without reimplementing workspace-glob resolution in a shell one-liner.

- [module:pnpm] Install-stamp idiom (`deps` recipe, `apps/web/Cookfile`): `pnpm install --frozen-lockfile --silent && mkdir -p build && echo ok > $<out>` mirrors the `dotnet build`/stamp pattern from Task 7 — `node_modules` is a large, pnpm-managed tree that is not itself a cook-declared output (an undeclared side-effect directory every downstream recipe implicitly depends on existing), the pnpm content-addressable store under `~/.local/share/pnpm/store` (or platform equivalent) is ambient machine state, and `--frozen-lockfile` is the discipline that makes the install deterministic *given* the lockfile rather than resolving ranges fresh. `pnpm-lock.yaml` is listed as an `ingredients` file so a lockfile edit invalidates the stamp; `local` disposition is correct for the same reason as the dotnet case — the stamp's provenance includes ambient machine/network state (registry fetch, local store population) that shouldn't be shared fleet-wide as if reproducible. A `cook_pnpm` module's `pnpm.install()` target-maker should own this whole shape (frozen-lockfile flag, stamp path, `local` disposition, lockfile-as-ingredient) so it isn't hand-copied per Cookfile the way the dotnet and rust toolchain idioms already are (see Task 3/7 findings above).

- [module:pnpm] Per-package dep topology is the sharpest gap this task surfaced: `typecheck: deps client ui menudata` lists all three builder recipes as dependencies purely for **ordering** (so `client`'s generated `src/menuClient.ts` + `dist/`, `ui`'s `dist/`, and `menudata`'s `app/src/menu.json` all exist on disk before any member's `tsc -p . --noEmit` runs, since `@plateboard/ui`'s tsc needs `@plateboard/client`'s `.d.ts` and `@plateboard/app`'s tsc needs `@plateboard/ui`'s `.d.ts`), not for cache coupling — and because the `typecheck` recipe body never references `$<client>`, `$<ui>`, or `$<menudata>` as placeholders (only `$<in.name>`/`$<in.dir>` from the `pnpm:packages` fan-out and `$<out>`), those dependency edges carry **zero fingerprint weight**. See the invalidation-edge evidence below: editing `packages/ui/src/board.ts` re-ran `ui`'s own build but left all three `typecheck` fan-out stamps `cached`, meaning `pnpm --filter @plateboard/ui exec tsc -p . --noEmit` was *not* re-run against the changed source even though the source is inside `packages/ui/src/*.ts` — a path that exists nowhere in `typecheck`'s own `ingredients` (only the `pnpm:packages` probe, which fingerprints the *package list*, i.e. `{name, dir}` tuples, not each package's source tree). This is real staleness risk, not just coarseness: `cook typecheck` can report all-cached green while a member's actual type errors from an edited file go undetected, because the *only* thing coupled to cook's fan-out unit fingerprint is "does this package still exist in the workspace list," not "did this package's sources change." A `cook_pnpm` module should derive the real package dependency graph (each member's `dependencies`/`devDependencies` restricted to workspace-linked siblings, resolvable from `pnpm-workspace.yaml` + each `package.json`, or via `pnpm ls -r --json` which already reports the graph) and use it to build **per-member** `ingredients` sets (a member's own `src/**` plus its transitive workspace-internal dependencies' `src/**`/`dist/**`), rather than the single coarse "depend on every builder, fingerprint on the member list only" shape used here. This is exactly the datapoint the task asked to record rather than hand-fix inline.

### [module:dotnet] (5)

- [module:dotnet] Fresh `Microsoft.NET.Sdk.Web` project targeting `net10.0` on this machine's SDK (10.0.109, host runtime `10.0.9`) fails `dotnet build` with `NETSDK1226: Prune Package data not found .NETCoreApp 10.0 Microsoft.AspNetCore.App` out of the box — no code involved, this is pure toolchain state. Root cause: the SDK's package-pruning feature cross-checks the installed `Microsoft.AspNetCore.App` shared-runtime version (10.0.9) against a matching `Microsoft.AspNetCore.App.Ref` targeting pack, but this machine only has that Ref pack available via NuGet cache at 10.0.0/10.0.4 (no 10.0.9 match), and no local `/usr/share/dotnet/packs/Microsoft.AspNetCore.App.Ref` at all (pacman's `dotnet-targeting-pack` only ships the `Microsoft.NETCore.App.Ref` pack, not the ASP.NET one). The error message names its own escape hatch: added `<AllowMissingPrunePackageData>true</AllowMissingPrunePackageData>` to `Api/Api.csproj`'s `PropertyGroup` (not in the task-provided template) and the build/test/run all succeeded cleanly. The downstream `Api.Tests.csproj`, despite also carrying a bare `<FrameworkReference Include="Microsoft.AspNetCore.App" />`, did **not** need the same property — the flag only had to be set on the project that owns the Web SDK. Worth flagging for Task 7's Cookfile / a `cook_dotnet` module: any Cookfile invoking `dotnet build`/`dotnet test` against `net10.0` ASP.NET Core projects on a still-preview-ish SDK install may need this property (or a corrected local pack install) to be portable across machines; a `cook_dotnet` toolchain probe should probably assert/normalize the SDK+Ref-pack pairing the same way `cook_cc`/`cook_rust` probes assert compiler/toolchain identity, rather than let each Cookfile author hand-add the workaround.

- [module:dotnet] Stamp-file pattern (`services/api/Cookfile`'s `build` recipe): `dotnet build` is not itself declarative or reproducible enough to be a cook output — it implicitly restores from NuGet on every invocation that lacks a satisfied lock (undeclared network I/O plus `~/.nuget` machine-state as an unsealed determinant), and MSBuild, not cook, owns the actual product artifacts under `Api/bin` and `Api/obj` (paths, layout, and staleness rules cook has no visibility into). The recipe therefore declares `cook "build/api-build.stamp" { dotnet build ... && echo ok > $<out> } local`: cook's own output is a one-line stamp, not the DLL. This is honest about what cook can promise (`bin`/`obj` are excluded from cook's model entirely, see below) but it opens a real correctness trap a `cook_dotnet` module must own: a warm cook cache (stamp present, hash matches, reported `cached`) says nothing about whether `Api/bin`/`Api/obj` actually exist on disk — e.g. after `git clone` into a fresh checkout with a *pre-warmed* `~/.cache/cook` or shared-store hit, cook would report the `build` recipe `cached` and skip re-running `dotnet build` entirely, leaving `bin/`/`obj/` never populated, and any downstream step that assumes MSBuild's own output directory exists (not just cook's declared `$<out>` stamp) would fail or silently use stale binaries. `dotnet test` also implicitly rebuilds (see below), which happens to paper over this for the `tests` recipe specifically since `dotnet test` re-invokes MSBuild itself — but that's incidental, not a guarantee, and a bare `dotnet run`/`dotnet Api/bin/Debug/net10.0/Api.dll` consumer would not get the same rescue. A `cook_dotnet` module's `dotnet.build()` target-maker should either (a) declare `bin`/`obj` as real cook outputs it manages end-to-end (losing the "MSBuild owns it" simplicity but closing the gap), or (b) document loudly and defensively that its stamp is a *build-happened* marker only, and any consumer must re-derive the real artifact path itself rather than trust cook's cache state as a proxy for `bin/`'s existence.

- [module:dotnet] `dotnet build`'s implicit restore is an unsealed, undeclared determinant: NuGet package resolution consults `~/.nuget/packages` (machine-global cache) and the network (nuget.org or whatever feeds `NuGet.Config` points at) on every restore that isn't already fully satisfied locally, and none of that is a cook `ingredient`, `seal`, or `probe` in this Cookfile — cook has no way to invalidate the `build` stamp if the resolved package graph changes (e.g. a floating version range resolves differently, or a package is yanked/republished) without a corresponding source-file change. The `local` disposition on `build` is the right disposition call given this (an artifact whose provenance includes ambient network/machine state should not be shared fleet-wide as if it were reproducible), but it doesn't fix the invalidation gap, only contains its blast radius to the producing machine. A `cook_dotnet` module should own this properly: ship a separate `restore` recipe/target-maker that seals a lock file (`packages.lock.json`, which `dotnet restore --use-lock-file` can produce and pin) as an `ingredient`, and have `build`/`test` invoke `dotnet build/test --no-restore` against that sealed, locked restore — turning an ambient network determinant into a declared file determinant, the same discipline `cook_cc`/`cook_rust` apply to compiler toolchains via `tools` probes.

- [module:dotnet] MSBuild's own incremental build (its `obj/*.cache`/timestamp bookkeeping under `Api/obj`) is a second, independent incrementality layer sitting *underneath* cook's — cook decides whether to re-run the `dotnet build` command at all (via the ingredients/seal fingerprint), and if it does re-run, MSBuild then separately decides internally what actually needs recompiling based on its own up-to-date checks over `obj/`. This mostly composes fine in the common case (cook re-runs the command, MSBuild does a fast incremental build inside that re-run and it's still much cheaper than `-v q`'s wall time suggests for a full rebuild), but it is a second cache with its own staleness rules layered under the first, invisible to `cook why`: a MSBuild-side false-incremental (stale `obj/` cache serving an out-of-date DLL despite cook believing the command "ran fresh") is a class of bug cook's model cannot see or account for. Not exercised as an actual failure here, but worth the module owning `dotnet clean`/`--no-incremental` as an explicit escape hatch rather than trusting MSBuild's own incrementality inside an already-incremental cook step.

- [module:dotnet] `bin/`/`obj/` exclusion: this repo's root `.gitignore` already excludes `bin/` and `obj/` globally (shared with the `apps/web`-style JS build-output exclusions), and this Cookfile never declares either directory as a cook `ingredients` glob or `cook` output — correctly, since both are MSBuild-owned scratch/product directories with their own (non-content-addressed, timestamp-heavy, machine-path-embedding) staleness model that would poison cook's cache if fingerprinted directly (e.g. `obj/*.cache` files embed absolute paths and timestamps that change on every restore regardless of source content). A `cook_dotnet` module should bake this exclusion in as a documented convention (never glob `bin`/`obj` into `ingredients`, never declare either as a `cook` output) rather than leave each Cookfile author to independently discover it.

### [module:rust] (2)

- [module:rust] cargo-target-dir vs cook-output copy idiom — `cook "build/menugen" { cargo build --quiet && mkdir -p build && cp target/debug/menugen $<out> } nondet` is boilerplate every Rust Cookfile will repeat: build into cargo's own `target/`, then manually `mkdir -p` + `cp` into cook's declared output path. A `cook_rust` module should ship a `rust.bin(name)` target-maker that owns the cargo invocation, target-dir plumbing, and the copy-to-`$<out>`, the same way `cook_cc` owns `cc.bin()`.

- [module:rust] toolchain probe boilerplate — `probe rust:tools\n    tools { cargo, rustc }` plus `seal rust:tools` on every recipe that compiles is hand-written per Cookfile. A `cook_rust` module should ship this probe pre-declared (module-namespaced, e.g. `rust:tools`) so consuming Cookfiles just `seal rust:tools` without redeclaring the producer.

### [module:python] (1)

- [module:python] Toolchain probe boilerplate mirrors the Rust case from Task 3: `probe py:tools { tools { python3 } }` + `seal py:tools` is hand-rolled per Cookfile here; a `cook_python` module should ship this pre-declared (`python:tools` or similar) so consumers just `seal python:tools`. More importantly, the probe as specified only content-hashes the `python3` binary found on `PATH` — it says nothing about which packages are importable in that interpreter. This recipe's codegen script does `import yaml` (PyYAML), and PyYAML is *not* an ingredient, not probed, and not sealed: it's an ambient, undeclared determinant of `build/menuClient.ts`'s content. If PyYAML's parsing/dumping behavior changed between versions (e.g. a `safe_load` edge-case fix, or a change in dict key ordering before that was Python-version-guaranteed), cook would produce a different output with an unchanged cache key and call it a hit — a real "the build is a lie" risk for any offline codegen recipe that imports third-party libraries. A `cook_python` module should own "codegen environment" identity end-to-end: not just `which python3` but a locked/venv'd interpreter + resolved dependency set (e.g. hash a `requirements.txt`/lockfile, or run inside a probed venv) so the full determinant set is captured, the same way `cook_cc`'s probes cover compiler + flags rather than just "some cc exists on PATH".

### [module:generic] (6)

- [module:generic] `contracts/Cookfile` is the second instance in this repo (after `tools/menugen`) of the pattern "static contract file (OpenAPI/YAML/JSON schema) → offline codegen script → generated source file consumed by another app". This is common enough (protobuf, OpenAPI, GraphQL SDL, JSON Schema-to-types) that it reads as a packageable target-maker: something like `codegen.from_spec(spec, generator_cmd, out)` that owns the ingredients-declaration/seal/mkdir-out boilerplate shown here, leaving the Cookfile author only the spec path, the generator invocation, and the output path. Right now this 4-line recipe is small enough that the boilerplate isn't painful, but it is exactly the shape that recurs.

- [module:generic] `MENU_JSON` env-var fixture-injection idiom (`test { MENU_JSON="$(realpath $<menugen.menu>)" dotnet test ... }`): worked exactly as designed, first try, no core-bug. `$<menugen.menu>` — a cross-Cookfile qualified dependency-output placeholder reaching into an `import`ed sibling Cookfile — resolved correctly inside a `test` step body (not just `cook` step bodies, confirming fixture `13-test-steps`'s `$<app>`-in-`test{}` pattern generalizes to cross-Cookfile refs too) to the *relative* path `../../tools/menugen/build/menu.json` (relative to `services/api`, the invoking Cookfile's directory — consistent with the `./$<build>` "always relative, never auto-prefixed" finding from Task 3). Wrapping it in `$(realpath ...)` at shell-invocation time, inline in the `MENU_JSON=` assignment, converts it to an absolute path before `dotnet test` forks its own subprocess (the xunit test host, whose cwd is `Api.Tests/bin/Debug/net10.0/`, several directories away from where the relative path would resolve) — exactly the trap the task called out, and exactly the fix. This is a clean, generalizable idiom for any cross-language fixture-passing case (a build/codegen artifact produced by one language's toolchain, consumed by a test suite in another): env-var injection at the `test`/`cook` step's shell prefix, `realpath`'d if the consumer's own subprocess changes cwd. Worth a `cook_generic`/testing-module helper (`test.with_env_from_dep(NAME, VAR)`) that owns the `realpath` wrapping so callers don't have to remember it's needed.

- [module:generic] Copy-artifact-into-consumer-source-tree idiom, two instances in this Cookfile: `client`'s `mkdir -p packages/client/src && cp $<contracts.gen> $<out>` (copying a codegen'd `.ts` file from another Cookfile's build dir into this package's own `src/` so its own `tsc` compiles it as first-party source) and `menudata`'s `mkdir -p app/src && cp $<menugen.menu> $<out>` (same shape, copying `tools/menugen`'s generated `menu.json` into `app/src/` so `resolveJsonModule` picks it up as a same-tree import). Both are "another Cookfile produced an artifact; this package needs it to physically live inside its own `src/` for the local toolchain (tsc rootDir, bundler resolution) to see it as first-party, not `node_modules`-linked" — a shape that will recur any time codegen output needs to be copied across a package boundary rather than published/linked as a dependency. A `cook_generic` helper (`fs.copy_into(dep_output, dest)`, owning the `mkdir -p $(dirname dest)` + `cp` pairing) would remove the repeated boilerplate; not filed as a `module:pnpm`-specific item because neither instance is pnpm-specific (the same shape would apply to any language's codegen-into-src idiom).

- [module:generic] Multi-output dist declaration vs stamp files: both `client`'s and `ui`'s tsc steps declare `cook "<dist>/x.js" "<dist>/x.d.ts" { tsc ... }` (the real two files tsc emits), rather than collapsing to a single stamp the way the dotnet/pnpm-install recipes do — and cook's stale-output reconciliation genuinely verified both declared paths exist after the run (confirmed by direct `ls`: `packages/client/dist/{menuClient.js,menuClient.d.ts}` and `packages/ui/dist/{board.js,board.d.ts}` all present after a cold `bundle` run). This is the *better* idiom when the tool's output set is small, static, and known in advance (unlike `dotnet build`'s deep, non-enumerable `bin`/`obj` tree) — it lets cook track the real artifacts instead of a proxy stamp, at the cost of the Cookfile author having to enumerate outputs by hand and keep the list in sync with the compiler's actual emit set (e.g. this list would need updating if `tsconfig.json` added `declarationMap` or `sourceMap`, which emit additional files tsc writes silently and cook wouldn't know to expect or verify). Worth a `cook_pnpm`/`cook_tsc` module owning `tsc.project(dir)` as a target-maker that introspects `tsconfig.json`'s `outDir`+`rootDir`+`declaration`/`sourceMap` flags to compute the exact expected output set rather than requiring each Cookfile author to hand-enumerate it.

- [module:generic] The `: $<dep> &&` no-op-reference idiom (applied to `apps/web/Cookfile`'s `ui`/`bundle`/`typecheck`/`smoke` bodies in this rework) is itself the workaround for the `[core-ergonomics]` header-only-dep finding above, and it is easy-to-forget, invisible boilerplate: nothing in the Cookfile surface or `cook`'s own diagnostics flags a `recipe A: B` header with no corresponding `$<B>` body reference as suspicious, so every module's target-makers (`cook_cc`, `cook_rust`, `cook_pnpm`, etc.) that fan out a chain of `recipe X: Y` dependency declarations must either (a) emit this `: $<dep> &&` prefix automatically on every generated body, or (b) core must close the gap directly (fold header deps into the cache key by default, or lint/warn on a declared-but-unreferenced dependency). Until one of those lands, an author hand-writing a Cookfile — exactly as `apps/web/Cookfile` was originally hand-written for this repo — WILL forget it on some recipe, and the failure mode is silent: a stale-but-green artifact, not a build error. Applying this fix made `typecheck`'s fan-out invalidation coarse (any one of `client`/`ui`/`menudata` changing now re-runs all 3 members, since the fan-out step can't cheaply distinguish "does *this* member actually depend on `ui`" without real per-package dependency-graph introspection) — that coarseness is already recorded as the `[module:pnpm]` "Per-package dep topology" finding above; the no-op-reference fix trades a false-negative (silently stale) for over-invalidation (coarse-but-sound), the correct trade for this rework, but it does not replace the real per-member-ingredients fix that finding calls for.

- [module:generic] Stamp-output dep edges are unfoldable when the stamp content is constant: `apps/web/Cookfile`'s `deps` recipe originally wrote `echo ok > $<out>` to `build/pnpm-install.stamp` — byte-identical on every successful `pnpm install`, regardless of what actually got installed. Even with every consumer folding `$<deps>` via the `: $<dep> &&` no-op idiom (see the entry above), a toolchain bump reachable only through `pnpm-lock.yaml` (e.g. a `typescript`/`esbuild` version bump) re-ran the `deps` unit (its own `ingredients` include the lockfile) but produced the same `ok` bytes, so early cutoff kept every downstream consumer (`client`, `ui`, `bundle`, `typecheck`) cached against a compiler/bundler version they never actually ran against — a stale-but-green dist built by an old compiler with a fresh-looking cache. The fix applied here: make the stamp's *content* a toolchain fingerprint (`sha256sum pnpm-lock.yaml; pnpm exec tsc --version; pnpm exec esbuild --version`) instead of a constant, so a real toolchain change produces different stamp bytes and correctly defeats early cutoff downstream. Any module shipping an install-stamp target-maker (`cook_pnpm`, `cook_dotnet`'s restore stamp, etc.) must emit meaningful, toolchain-sensitive stamp content, not a constant `echo ok` — a constant stamp makes its own dependency edge structurally unfoldable no matter how carefully consumers reference it.

## Verification evidence

Index: Bar 1 (cold build) and Bar 2 (no-op rebuild) are in the Task 10 block; Bar 3 (edge proofs + `cook why`) is in the Task 11 block; Bar 4 (polyglot test + per-suite caching) spans the Task 10 pre-check and the Task 11 block; Bar 5 (cold-build wall time, 5.13s, inside the ~2min budget) is Task 10 / Bar 1.

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
# [Superseded/refined later: this generalization holds for TEST-step units only; cook-step
#  units fold upstream output CONTENT and do get early cutoff — see the test-vs-cook
#  dependency-fold [core-bug] entry in the Log, which reconciles this observation.]

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

### Task 9 — apps/web/Cookfile

```
$ cd apps/web
$ rm -rf packages/client/src packages/client/dist packages/ui/dist app/dist app/src/menu.json build
$ cook bundle
  contracts.gen            queued  (2 nodes)
  deps                     queued  (2 nodes)
  menugen.build            queued  (2 nodes)
  client                   queued  (2 nodes)
  menugen.menu             queued  (1 nodes)
  ui                       queued  (2 nodes)
  menudata                 queued  (1 nodes)
  bundle                   queued  (2 nodes)
  contracts.gen/$probe:py:tools                         0.00s
  contracts.gen/menuClient.ts                           cached
  contracts.gen            done     (1/2 cached)             1.60s
  menugen.build/$probe:rust:tools                       0.00s
  menugen.build/menugen                                 cached
  menugen.build            done     (1/2 cached)             0.02s
  menugen.menu/menu.json                               cached
  menugen.menu             done     (1/1 cached)             0.00s
  deps/$probe:web:tools                        0.00s
  menudata/menu.json                               0.00s
  menudata                 done     (1/1)                    0.00s
  deps/pnpm-install.stamp                      0.00s
  deps                     done     (2/2)                    0.34s
  client/menuClient.ts                           0.00s
  client/menuClient.js                           0.00s
  client                   done     (2/2)                    0.49s
  ui/$probe:web:tools                        cached
  ui/board.js                                0.00s
  ui                       done     (1/2 cached)             0.47s
  bundle/$probe:web:tools                        cached
  bundle/bundle.js                               0.00s
  bundle                   done     (1/2 cached)             0.24s
cook done in 6.16s (14 nodes, 1 cached recipes, 8 done)
# both declared outputs of every multi-output tsc step confirmed present via `ls`:
# packages/client/dist/{menuClient.js,menuClient.d.ts}, packages/ui/dist/{board.js,board.d.ts}

$ cook typecheck
  ...
  typecheck/client.stamp                            0.00s
  typecheck/app.stamp                               0.00s
  typecheck/ui.stamp                                0.00s
  typecheck                done     (3/3)                    0.49s
cook done in 3.55s (15 nodes, 5 cached recipes, 8 done)
# 3 fan-out units confirmed on disk at:
#   build/typecheck/app.stamp
#   build/typecheck/packages/client.stamp
#   build/typecheck/packages/ui.stamp
# (see [core-ergonomics] label-truncation finding above -- the live log shows the short
# names "client.stamp"/"ui.stamp"/"app.stamp", not the actual nested "packages/..." paths)

$ cook bundle && cook typecheck   # all cached, second pass
  ... bundle: (14 nodes, 6 cached recipes, 8 done), all leaf nodes "cached"
  ... typecheck: (15 nodes, 6 cached recipes, 8 done), all 3 stamps "cached"

$ cook test
running tests
test check@15 [menu.toml] ... ok (cached)
test smoke@41 ... ok
test result: ok. 2 passed (1 cached); finished in 4.6s
# workspace-wide aggregation again pulled in menugen's `check` test recipe, consistent
# with the Task 7 finding; `smoke` itself was NOT cached on this first `cook test`
# invocation for apps/web (no prior test-scoped run had ever recorded it), which is
# expected -- `bundle` being cached does not imply the `smoke` *test* unit was cached.

$ cook test   # re-run, no edits
running tests
test check@15 [menu.toml] ... ok (cached)
test smoke@41 ... ok (cached)
test result: ok. 2 passed (2 cached); finished in 4.5s
```

Invalidation edge — `packages/ui/src/board.ts` (append `export const BOARD_VERSION = 1;`):

```
$ echo "export const BOARD_VERSION = 1;" >> packages/ui/src/board.ts
$ cook bundle
  ...
  ui/board.js                                0.00s
  ui                       done     (1/2 cached)             0.46s
  bundle/bundle.js                               cached
  bundle                   done     (2/2 cached)             0.00s
# ui rebuilt (correctly -- packages/ui/src/*.ts is its own glob ingredient);
# bundle stayed cached even though it depends on ui (recipe header) -- bundle's body
# never references $<ui>, so no fingerprint coupling exists

$ cook typecheck
  ...
  typecheck/client.stamp                            cached
  typecheck/ui.stamp                                cached
  typecheck/app.stamp                               cached
  typecheck                done     (3/3 cached)             0.00s
# all 3 fan-out stamps stayed cached, including the "ui" member itself -- typecheck's
# only ingredient is the pnpm:packages probe (fingerprints the {name,dir} list, not
# each member's source tree), so editing packages/ui/src/board.ts is invisible to it

# direct confirmation the cache was actually stale, not just quiet:
$ stat -c '%Y %n' app/dist/bundle.js packages/ui/dist/board.js \
    build/typecheck/app.stamp build/typecheck/packages/ui.stamp
1783697841 app/dist/bundle.js            # older -- NOT rebuilt
1783697937 packages/ui/dist/board.js     # newer -- rebuilt
1783697882 build/typecheck/app.stamp     # predates the board.js rebuild
1783697882 build/typecheck/packages/ui.stamp   # predates the board.js rebuild
$ grep -c BOARD_VERSION packages/ui/dist/board.js   # 1 -- new symbol present
$ grep -c BOARD_VERSION app/dist/bundle.js          # 0 -- stale bundle never saw it

$ git checkout packages/ui/src/board.ts
$ cook bundle
  ...
  ui/board.js                                cached   # revert hit the shared store immediately
  bundle/bundle.js                               cached
  bundle                   done     (2/2 cached)             0.00s
$ git status --short   # clean
```

See the "Fan-out invalidation observation" and "Per-package dep topology" log entries above for the
analysis: recipe-level `: dep` headers are ordering-only edges unless the body references `$<dep>`
directly; fan-out member identity (the `pnpm:packages` probe) fingerprints the *member list*, not
each member's source tree, so this Cookfile's dependency shape is deliberately coarse and can report
false-cached on real per-package source edits.

### Task 10 — root Cookfile

#### Bar 1 — cold build

```
$ git add Cookfile   # protects the new untracked root Cookfile from the git-clean step below
$ rm -rf ~/.cache/cook
$ git clean -xfdn   # eyeballed: only gitignored build dirs (.cook, node_modules, bin/obj,
                     # target, dist) -- no tracked file listed, new Cookfile excluded (staged)
$ git clean -xfd
Removing .cook/  apps/web/.cook/  apps/web/app/dist/  apps/web/app/node_modules/
Removing apps/web/app/src/menu.json  apps/web/build/  apps/web/node_modules/
Removing apps/web/packages/client/dist/  apps/web/packages/client/src/
Removing apps/web/packages/ui/dist/  apps/web/packages/ui/node_modules/  build/
Removing contracts/.cook/  contracts/build/  services/api/.cook/
Removing services/api/Api.Tests/bin/  services/api/Api.Tests/obj/
Removing services/api/Api/bin/  services/api/Api/obj/  services/api/build/
Removing tools/menugen/.cook/  tools/menugen/build/  tools/menugen/target/

$ time COOK=/home/alex/dev/cook/cli/target/debug/cook $COOK build
  api.build                queued  (2 nodes)
  web.contracts.gen        queued  (2 nodes)
  web.deps                 queued  (2 nodes)
  web.menugen.build        queued  (2 nodes)
  web.client               queued  (3 nodes)
  ... (11 recipes, 24 nodes total queued)
  web.contracts.gen        done     (2/2)                    0.04s
  web.menugen.build        done     (2/2)                    0.35s
  web.menugen.menu         done     (1/1)                    0.04s
  web.menudata             done     (1/1)                    0.00s
  web.deps                 done     (2/2)                    0.77s     # fresh pnpm install
  web.client               done     (3/3)                    0.51s
  web.ui                   done     (2/2)                    0.55s
  web.bundle                done     (2/2)                    0.28s
  web.typecheck             done     (4/4)                    0.56s
  [api.build/probe:dotnet:tools] Build succeeded. 0 Warning(s), 0 Error(s), Time Elapsed 00:00:04.64
  api.build                done     (2/2)                    4.81s     # fresh dotnet build (obj/ absent)
  build/$probe:stack:versions                   0.00s
  build/manifest.txt                            0.00s
  build/manifest.lua.txt                        0.00s
  build                    done     (3/3)                    0.28s
cook done in 5.10s (24 nodes, 0 cached recipes, 11 done)
$COOK build  8.57s user 0.98s system 185% cpu 5.134 total
```

Well under the ~2 min budget (5.13s wall / ~9.5s CPU across 32 cores). `node_modules` (2 top-level + app), `Api/bin`+`Api/obj`, and `target/debug/menugen` all confirmed freshly regenerated (menugen has zero external crates, so its "cold" cargo compile is legitimately sub-second — not a caching artifact, verified via `Cargo.toml`). `build/manifest.txt` content spot-checked correct (`dotnet 10.0.109`, `node v22.23.1`, `pnpm 10.33.0`, `cargo 1.93.1`, `python 3.14.6`, plus the resolved `menugen.menu`/`web.bundle` cross-Cookfile placeholder paths).

#### Bar 2 — no-op rebuild

```
$ $COOK build
  web.contracts.gen/menuClient.ts                           cached
  api.build/api-build.stamp                         cached
  web.menugen.build/menugen                                 cached
  web.menugen.menu/menu.json                               cached
  web.menudata/menu.json                               cached
  web.deps/pnpm-install.stamp                      cached
  web.client/menuClient.ts                           cached
  web.client/menuClient.js                           cached
  web.ui/board.js                                cached
  web.bundle/bundle.js                               cached
  web.typecheck/client.stamp (x3, mislabeled -- see label-truncation [core-ergonomics] entry above)  cached
  build/$probe:stack:versions                   cached
  build/manifest.txt   (x2, second is actually manifest.lua.txt mislabeled)  cached
cook done in 0.17s (24 nodes, 3 cached recipes, 11 done)
```

Every non-probe unit cached; wall time dropped 5.13s -> 0.17s. `tools {}`-kind probes (`$probe:*:tools`) show `0.00s` rather than `cached` on both runs, including this no-op one -- expected per §{cat.probes} `tools {}` semantics (CS: "the hash is both the probe value and its re-run trigger" -- the probe body always re-executes to recompute the binary hash; only *downstream* units get to be `cached` when the hash is unchanged). Not a new finding. The two live-log mislabelings (`web.typecheck`'s three fan-out members all printing the same truncated stamp name; the root `build` recipe's second `cook` step printing `manifest.txt` instead of `manifest.lua.txt`) are the same cosmetic label-truncation `[core-ergonomics]` finding already logged for Task 9 -- both `build/manifest.txt` and `build/manifest.lua.txt` were independently confirmed correct and unchanged on disk (`ls -la` mtimes identical pre/post this no-op run).

#### Bar 4 pre-check — root `cook test`

```
$ $COOK test
running tests
test web.menugen.check@15 [menu.toml] ... ok
test web.smoke@44 ... ok
test api.tests@14 ... ok

test result: ok. 3 passed; finished in 2.3s
```

Root `cook test` (bare, no scope arg) DOES aggregate every test recipe reachable transitively through all four imports in one invocation: `menugen.check` (rust, via `web`'s `menudata: menugen.menu` edge), `web.smoke` (the pnpm/esbuild bundle smoke test), and `api.tests` (xunit). This matches the workspace-wide-by-default behavior already documented for sub-tree `cook test` invocations (Task 7/9 findings) -- scoping up to the workspace root doesn't change the aggregation rule, it was already whole-workspace regardless of which Cookfile issued the bare command. No `[core-ergonomics]` filed since this is the expected/desired behavior for a root aggregation point. Per the known bare-`cook test` local-index poisoning quirk, no further cache conclusions are drawn from any run after this one -- Bars 1 and 2 above were both captured before this `cook test` invocation.

### Task 11 — edge proofs (Bars 3+4)

Baseline: repo was already fully settled from Task 10 (`git status` clean at task start). `$COOK build` twice in a row from root both reported `cook done in 0.17s (24 nodes, 3 cached recipes, 11 done)` — fully cached, confirming the settled starting point before any edit.

#### Edge 1 — contract edit (`contracts/menu-api.yaml`)

Edit: added an optional `description: string` property to the `MenuItem` schema.

Observed invalidation set (`$COOK build`, full transcript in `/tmp/edge-contract.txt`):

| unit | result |
|---|---|
| `web.contracts.gen` (menuClient.ts) | **re-ran** |
| `web.client` (both cook steps: cp + tsc) | **re-ran** |
| `web.ui` (tsc step) | **re-ran** (recompiled — client dist content changed) |
| `web.bundle` | **cached** (1/2 cached; only the `tools{}` probe re-executes, bundle.js itself hit) |
| `web.typecheck` (3 fan-out members + probe) | **re-ran**, all 4/4 |
| `api.build` | cached |
| `web.menugen.*`, `web.menudata`, `web.deps` | cached (unaffected) |
| `build` (root manifest) | cached (3/3 — both `manifest.txt` and `manifest.lua.txt` unchanged) |

`cook done in 1.61s (24 nodes, 3 cached recipes, 11 done)`.

Matches the expected model exactly, **including the early cutoff**: `packages/client/dist/menuClient.d.ts` gained `description?: string;` (verified: `grep -n description apps/web/packages/client/dist/menuClient.d.ts` → `description?: string;`), which forced `ui`'s tsc step to re-run (it folds client dist content) — but `board.ts` never references `description`, so `packages/ui/dist/board.js`/`.d.ts` came out byte-identical, and `web.bundle` correctly stayed cached off that unchanged content (`grep -c description apps/web/app/dist/bundle.js` → `0`). Root `build` also stayed cached since neither `web.bundle` nor `menugen.menu` content changed. Re-ran `$COOK build` again after: fully cached, settled. Edit kept.

#### Edge 2 — menugen source edit (`tools/menugen/src/main.rs`)

Edit: `to_json` now also emits a `"currency": "USD"` field per menu item.

`cook why` captured **before** building (read-only; full transcripts `/tmp/why-root.txt`, `/tmp/why-bundle.txt`) — see the condensed excerpt below and the new `[core-ergonomics]` Log entry above: the direct producer (`menugen.build`) correctly reported `[MISS (shared)]`, but every downstream unit in the same report (`menu`, `menudata`, `bundle`, root `build`) still reported `[HIT (local)]`, because `why` evaluates each unit against inputs *currently on disk*, not a simulated post-rebuild state.

Observed invalidation set (`$COOK build`, full transcript `/tmp/edge-menugen.txt`):

| unit | result |
|---|---|
| `web.menugen.build` (cargo) | **re-ran** |
| `web.menugen.menu` (menu.json) | **re-ran** (now has `currency`) |
| `web.menudata` (copy) | **re-ran** |
| `web.bundle` | **re-ran** (2/2 — content changed via menudata) |
| `web.typecheck` (3 members) | **re-ran**, 4/4 (folds menudata content) |
| `build/manifest.txt` (shell step) | **re-ran** (folds bundle + menu content) |
| `build/manifest.lua.txt` (lua step) | cached (only folds the `stack:versions` probe, independent of manifest.txt/menu content) |
| `web.contracts.gen`, `web.client`, `web.ui`, `web.deps`, `api.build` | cached (unaffected) |

`cook done in 0.92s (24 nodes, 0 cached recipes, 11 done)`. Verified `tools/menugen/build/menu.json` contains `"currency": "USD"` per item, and `grep -c currency apps/web/app/dist/bundle.js` → `3`.

Then `$COOK test` (full transcript `/tmp/edge-menugen-test.txt`):

```
running tests
test web.menugen.check@15 [menu.toml] ... ok
test web.smoke@44 ... ok
test api.tests@14 ... ok

test result: ok. 3 passed; finished in 2.1s
```

None of the three carry a `(cached)` tag — **all three re-ran**, matching the expected model: `menugen.check` folds `$<menugen.build>` execution (menugen.build re-ran), `api.tests` folds `$<menugen.menu>` execution (menu re-ran), `web.smoke` folds `$<bundle>` execution (bundle re-ran). All passed — confirms the dotnet `MenuItem` deserializer (`PropertyNameCaseInsensitive`) silently ignores the unknown `currency` field rather than failing. Re-ran `$COOK build`: fully cached, settled. Edit kept.

#### Edge 3 — ui package edit (`apps/web/packages/ui/src/board.ts`)

Edit: header string `"== PLATEBOARD =="` → `"== PLATEBOARD v2 =="` (a used symbol, survives esbuild tree-shaking).

Observed invalidation set (`$COOK build`, full transcript `/tmp/edge-ui.txt`):

| unit | result |
|---|---|
| `web.ui` | **re-ran** (2/2) |
| `web.bundle` | **re-ran** (2/2) — verified `grep -c "PLATEBOARD v2" apps/web/app/dist/bundle.js` = `1` |
| `web.typecheck` (3 members) | **re-ran**, 4/4 |
| `build/manifest.txt` | **re-ran** (folds bundle content) |
| `build/manifest.lua.txt` | cached |
| `web.menugen.*`, `web.contracts.*`, `api.*`, `web.client`, `web.deps` | cached |

`cook done in 1.16s (24 nodes, 2 cached recipes, 11 done)`. Matches the expected model exactly.

Then `$COOK test` (full transcript `/tmp/edge-ui-test.txt`):

```
running tests
test menugen.check@15 [menu.toml] ... ok (cached)
test api.tests@14 ... ok (cached)
test web.smoke@44 ... ok

test result: ok. 3 passed (2 cached); finished in 0.2s
```

`web.smoke` re-ran (no `(cached)` tag, bundle re-executed); `api.tests` and `menugen.check` stayed cached (their deps — `menugen.menu`, `menugen.build` — did not re-execute for this edit). Exactly the expected model. Re-ran `$COOK build`: fully cached, settled. Edit kept.


#### Bar 4 — per-suite test caching

From the fully settled state (all three edits kept, all prior builds green), `$COOK test` again:

```
running tests
test menugen.check@15 [menu.toml] ... ok (cached)
test api.tests@14 ... ok (cached)
test web.smoke@44 ... ok (cached)

test result: ok. 3 passed (3 cached); finished in 0.2s
```

All three suites (rust/menugen, dotnet/xunit, node/esbuild-smoke) report `(cached)` — one polyglot `cook test` invocation, three independently-tracked per-suite cache hits, none re-executed. Combined with the exact re-run/cached splits observed in Edges 1-3 above (each edit re-ran precisely the suites whose upstream execution-folded dependency actually re-executed, and left the rest cached), this is the Bar 4 evidence: per-suite caching is real and precise at unit granularity, not an all-or-nothing "did anything change" gate.


#### `cook why` key-attribution excerpt

Condensed from `/tmp/why-root.txt` (captured before the menugen source edit was built), showing the direct MISS at the true edit site and a downstream unit's stale-but-consistent HIT:

```
build :: build/menugen@2d06800538d394c2 [MISS (shared)]  key 39060791c5abb62e3531f16a6d9dc4199634cdeb89ecf5105f14db60b88cd122
  command_hash      1aeee7814cf4845c
  env_contribution  2d06800538d394c2
  seal_contribution eb17c9009cb2f65b
  inputs:
    Cargo.toml  b822982cadd30eee
    src/main.rs  62bc77a1eaeaac4c
  outputs:
    build/menugen
  sealed probes:
    rust:tools = { "cargo": {"path": "/usr/bin/cargo", ...}, "rustc": {"path": "/usr/bin/rustc", ...} }
  shared-miss diff: no producer manifest published for this key

menu :: build/menu.json@2d06800538d394c2 [HIT (local)]  key 9b88d2a1fdbce829abc1517e9ba9863837931221c61526d29b2e452ef84f8134
  command_hash      41d0d6c7cf5fc487
  seal_contribution 0000000000000000
  inputs:
    build/menugen  3707fb34781d0e70   # <- still the OLD menugen hash; menu itself hasn't re-derived yet
    menu.toml  a16173fa3b4c209a
  outputs:
    build/menu.json
```

`src/main.rs`'s input hash (`62bc77a1eaeaac4c`) differs from any recorded key for the `menugen.build` unit, producing a direct `MISS (shared)` with `no producer manifest published for this key`. `menu`, one edge downstream, still reports `HIT (local)` because its own `inputs:` list still shows `build/menugen`'s *current on-disk* content hash (`3707fb34781d0e70`) — `why` does not simulate what `menugen`'s rebuilt output hash will be, only what the graph looks like right now. See the new `[core-ergonomics]` Log entry above for the full analysis and the resulting guidance (`cook why` gives confirmed direct cause, not full downstream blast radius).
