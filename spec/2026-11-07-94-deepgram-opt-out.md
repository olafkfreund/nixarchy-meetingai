---
status: draft
issue: 94
intent: intent/2026-11-07-94-deepgram-opt-out.md
---

# Spec: Deepgram never keeps call audio to improve its models

## Design

Send `mip_opt_out=true` on every request the built-in `deepgram-stt` engine makes, whatever the region. The region keeps choosing the host, and nothing else.

**Code, `src/speech/stt/transcription.rs`:**
- `deepgram_batch_url` (:365): drop the `eu` variable (:372) and append `mip_opt_out=true` unconditionally (:383–385).
- `live_url` (:814): append `&mip_opt_out=true` unconditionally (:824–826). The `eu` parameter stays, for the host only.
- `connect_config_with` (:855; its `eu` test is at :890) keeps passing `eu`; no change.
- Custom records (`transcribe_custom`, called at :170 and defined at :277; custom `stream_url`, :865) are untouched, as the intent decided. Its raw-wav branch (:288) adds only `model` and `language`.
- One `mip_opt_out` value only: a built-in engine's `base_url` is set by the app, either the provider's endpoint or the EU host (`src/daemon/session.rs:1570`), never by the user. So no stored URL can already carry `mip_opt_out=false`, and no de-duplication is needed.

**Tests, same file:**
- `live_url_checks_model_and_region` (:1545): add an assert that the US URL, `live_url("", None, false)`, contains `mip_opt_out=true`. The EU assert stays.
- New `batch_url_opts_out_on_both_regions`: `deepgram_batch_url("https://api.deepgram.com/v1/listen", "nova-3", None)` and the EU host both have `mip_opt_out=true` in their query.
- Both fail without the change: today the US URLs have no `mip_opt_out`.

**Docs:** the wording changes from "EU opts out" to "both regions opt out; EU keeps processing in the EU":
- `nix/hm-module.nix:233`, the `deepgram.region` description. Then regenerate the README options block (`flake.nix:29`, the `optionsDoc` between the nix-options markers) so `README.md:509` follows.
- `README.md:184`, the provider table.
- `README.md:202`, the Norwegian paragraph.
- `nix/hm-module.nix:135`, the warning: keep "eu is advised", since it still controls where audio is processed.
- Add one sentence near :184 saying a custom speech record sends only what its owner configured, so it needs `mip_opt_out=true` in its own URL to opt out.

## Alternatives rejected

- **Change the default region to EU.** This changes where people's audio is processed without asking them, and US users might see more latency. The opt-out alone fixes the retention.
- **A setting to choose whether to opt out.** Nobody asked for it, and keeping call audio for a vendor's training isn't something the app should offer by default.
- **Add the opt-out to custom records too.** A custom record can point at any server, and adding a query to someone else's URL could break it. Out of scope, per the intent.

## Risks

- **Price:** Deepgram's page doesn't say whether opting out costs more. Its pricing page lists one price. The PR says this wasn't confirmed with Deepgram, as decided.
- **Rejected requests:** if Deepgram refused `mip_opt_out` on some plan, US requests would fail. Their docs describe it as a per-request option for API users (terms 3.3), and EU requests already send it, so this is unlikely. Untested live: no Deepgram call is made.
- **Hosts:** p620 uses `parakeet-local`, so nothing changes in use there. Deploying isn't needed to make use of it, but the next routine deploy carries it.

## Verification

- The two tests above fail before the change and pass after it, checked by removing the change.
- The step check: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` in `nix develop`.
- `nix flake check`, which includes the README options check.
- `rg -n 'model improvement|opts? out' README.md nix/` lists every mention, and each one reads correctly for both regions, including the provider table at `README.md:184`. `nix flake check` covers only the generated options block (`flake.nix:365`), so the table is checked this way.
