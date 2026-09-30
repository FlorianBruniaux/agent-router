# Third-party license evidence

The project license, `FSL-1.1-ALv2` (Functional Source License 1.1, Apache 2.0
Future License), is in `LICENSE.md` at the repository root, with `NOTICE`. The
text is the official template with only the copyright line filled in. It is
not OSI approved, and each version converts to Apache 2.0 two years after
publication.

The source exporter preserves dependency license and notice texts separately
under `licenses/dependencies/` inside each generated archive. Their declarations
and byte provenance are recorded in `LICENSE-INVENTORY.json`. This evidence does
not replace a review of license compatibility for the chosen distribution.

The supplemental `difflib` 0.4.0 license comes from its
[matching upstream commit](https://github.com/DimaKudosh/difflib/blob/f035fb8e656f27119e23eca9d5b996df14f3885e/LICENSE).
All seven packaged Rust/original-manifest files match that commit's Git blobs.
The text's upstream attribution is preserved as published, without substituting
the crate author's name. The exact crate, blob and text hashes are recorded in
`third-party/evidence.json`.
