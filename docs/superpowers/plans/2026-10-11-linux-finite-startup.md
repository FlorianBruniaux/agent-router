# Linux finite-client startup ownership

Scope: replace the Linux diagnostic's ad-hoc bootstrap with producer-owned state, then qualify exactly that composition at 8 GiB. Existing durable approval and native admission remain separate gates. No global configuration, credentials, real provider turn or helper limit change.

## Task 1: bind startup state before effects

- [x] Add contract tests proving generated private SQLite/installation state, retained ownership, changed/symlink/replaced-inode refusal, approval rebinding and legacy proof refusal.
- [x] Observe RED before implementation. Expected: the startup contract is unavailable, then concrete corrupted states are refused.
- [x] Extend the existing private state producer using HostState and held identities. Manifest-only v2 stays compatible; owned copies advance to v4 and require fresh proof. Native invocation previews remain nonserializable authority and never become Command.
- [x] Test the real owned-state/validation paths and keep 8 GiB resource binding.

## Task 2: qualify the production producer under Linux

- [x] Switch the opt-in controller to the new producer; remove its bootstrap mutation.
- [x] Use a new dated evidence collector, preserving #171 evidence unchanged.
- [x] Rebuild the controller offline, pinned images, 8 GiB and no host mounts. Client/native/helper/wrapper remain pinned; qualify normal/deadline/EOF/helper death and independent heartbeat teardown.
- [x] Refuse native GO from a component result; report the exact remaining runtime/backend/admission gate.

## Task 3: verify and deliver

- [ ] Run full local quality, Linux producer/client tests and negative evidence tests with source hashes.
- [ ] Independent whole-branch review, fix concrete blockers with RED/GREEN evidence.
- [ ] Signed scoped commit, push and PR. No automatic merge of the new PR.

Spec: ADR 0028/0029 and the native boundary stated in docs/bench/linux-copied-client-2026-10-11/README.md. Retain current approval/proof/store contracts, requested 8589934592-byte limit, broker ownership and source/copy freshness. No store/observation or router-types format changes.
