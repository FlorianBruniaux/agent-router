# Third-party license evidence

The project license texts, `LICENSE-MIT` and `LICENSE-APACHE`, live at the
repository root so that GitHub detects them. The workspace declares
`MIT OR Apache-2.0`. `LICENSE-MIT` uses the contributor
attribution authorized for this project, `Agent Router contributors`.
`LICENSE-APACHE` contains the standard Apache License 2.0 terms. Its SHA-256 is
`62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a`.

The Apache text was obtained from the `serde` 1.0.229 package archive after its
SHA-256 matched Cargo.lock. Only the standard license text was copied; no Serde
implementation was copied. The upstream archive checksum at verification was
`4148590afebada386688f18773da617792bf2ef03ffc1e4cbd2b1d45b023e0ba`.

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
