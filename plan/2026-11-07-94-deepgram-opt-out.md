---
status: approved
issue: 94
spec: spec/2026-11-07-94-deepgram-opt-out.md
coders:
  1: codex   # small change, with tests that must fail without it
  2: coder   # wording across README and the Nix option, kept consistent
reviewers:
  - codex    # read-only: any Deepgram request path still without the opt-out
  - agy      # read-only: tests really fail without the change; the docs read right for both regions
---

# Plan: Deepgram never keeps call audio to improve its models

Branch `fix/94-deepgram-opt-out`, worktree `../meetingai-94`. Line numbers are from `origin/main` at `202995a`; find each place by its code.

## Decisions carried from the approved spec

- Every request the built-in `deepgram-stt` engine sends, batch or live, US or EU, carries `mip_opt_out=true`. The region still chooses the host, and nothing else.
- Custom speech records (`transcribe_custom`, custom `stream_url`) are untouched. The README says they send only what their owner configured.
- **One `mip_opt_out` value only. This corrects the spec, found in plan review:** the spec said a built-in engine's `base_url` is never set by the user, but a saved `deepgram-stt` credential carries its own `base_url` (`src/core/security/credentials.rs:28`, loaded at `src/daemon/session.rs:1554`). Its query is kept: batch parses it (`transcription.rs:216`), live joins it (`:906`). So any existing `mip_opt_out` is removed before `true` is added.
- No Deepgram call, no sign-up. The PR says opting out wasn't checked against Deepgram's pricing, which lists one price.
- The default region stays `us`. No new setting.

## Steps

1. **Code and tests, `src/speech/stt/transcription.rs`.**
   - New helper, near `deepgram_batch_url`:
     ```rust
     /// A stored endpoint may carry its own `mip_opt_out`; the app always sends one, `true`.
     fn drop_opt_out(url: &mut reqwest::Url) {
         let kept: Vec<(String, String)> = url
             .query_pairs()
             .filter(|(k, _)| k != "mip_opt_out")
             .map(|(k, v)| (k.into_owned(), v.into_owned()))
             .collect();
         url.set_query(None);
         if !kept.is_empty() {
             url.query_pairs_mut().extend_pairs(kept);
         }
     }
     ```
   - `deepgram_batch_url` (:365): call `drop_opt_out(&mut url)` after the parse. Remove `let eu = …` (:372) and the `if eu` around `append_pair("mip_opt_out", "true")` (:383–385), so it's always appended.
   - `live_url` (:814): append `&mip_opt_out=true` always (:824–826). Keep the `eu` parameter for the host.
   - `connect_config_with`, the stored-URL branch (:895): call `drop_opt_out(&mut target)` right after `target` is parsed, before its query is joined (:906).
   - `live_url_checks_model_and_region` (:1545): assert `live_url("", None, false)` contains `mip_opt_out=true`. Keep the EU asserts.
   - New `#[test] fn batch_url_opts_out_on_both_regions`:
     - `deepgram_batch_url("https://api.deepgram.com/v1/listen", "nova-3", None)` and the same with `https://api.eu.deepgram.com/v1/listen` both have the query pair `mip_opt_out=true`. Check with `url.query_pairs()`, not a substring.
     - With `https://api.deepgram.com/v1/listen?mip_opt_out=false&tag=x`, the only `mip_opt_out` pair is `true`, and `tag=x` is kept.
   - New `#[test] fn drop_opt_out_keeps_other_pairs`: `…?a=1&mip_opt_out=false&b=2` becomes `…?a=1&b=2`, and `…?mip_opt_out=false` has no query left.

   → verify: `nix develop -c bash -c 'cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check'`. Then each removal must fail a test:
     - in `deepgram_batch_url`, put back `let eu = url.host_str() == Some("api.eu.deepgram.com");` and its `if eu`, together;
     - in `live_url`, put back its `if eu`;
     - remove the `drop_opt_out` call in `deepgram_batch_url`.

     - remove the `drop_opt_out` call in the live stored-URL branch.

     **Deviation, from the final Opus review:** the live stored-URL call is tested after all. `configured_stream_keeps_the_supplied_host` already opens a local socket on that branch; its stored URL now carries `mip_opt_out=false`, and the handshake asserts one `mip_opt_out=true` and that `tenant=two` is kept. `drop_opt_out` also returns early when no `mip_opt_out` is present, so other stored queries keep their exact encoding.
   Traps:
   - Before cargo, run `find target/debug target/release -maxdepth 2 -name 'lib*.so*' ! -perm -u+w -delete`.
   - Codex's sandbox can't run nix or cargo: the lead runs the check.

2. **Docs.**
   - `nix/hm-module.nix:233`: the `deepgram.region` description becomes "Deepgram API region: `eu` processes audio in the EU. Both regions opt out of model improvement."
   - `README.md:509`: the generated options block. Run `nix build .#docs` and paste the result between `<!-- nix-options:begin -->` and `<!-- nix-options:end -->` (`flake.nix:374`). The flake check compares the two.
   - `README.md:184`, provider table: `deepgram.region = "eu"` "uses `api.eu.deepgram.com`". Every request opts out of model improvement.
   - `README.md:202`: replace "only that region opts out of model improvement" with: set `deepgram.region = "eu"` to keep processing in the EU. Every request opts out of model improvement.
   - After the provider table, one sentence: a custom speech record sends only what you configured, so add `mip_opt_out=true` to its URL to opt out with Deepgram.
   - `nix/hm-module.nix:135`: unchanged ("eu is advised" still holds, for where audio is processed).

   → verify: `nix flake check`. Then read every hit of `rg -n 'model improvement|opts? out' README.md nix/`: each must be right for both regions.
   Traps: don't touch `docs/eval/`, which records what was true on 2026-11-05.

## Tests

- Step 1's check, and the two removals that each fail a test.
- `nix flake check` passes.
- `nix develop .#shell -c bash shell/tests/ci.sh` passes (no shell change; it guards against surprises).

## Rollback

Revert the PR's merge commit. Requests go back to opting out on EU only. No data or setting to undo.

## Deploy

p620 uses `parakeet-local`, so nothing changes in use there. It ships with the next routine meetingai bump; no separate deploy.
