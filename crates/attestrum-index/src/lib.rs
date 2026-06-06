//! `attestrum-index` — derived fuzzy-lookup sidecar indexes (v1.1).
//!
//! Replaces `attestrum prove`'s scan-every-leaf fuzzy paths with an indexed
//! candidate lookup. The index persists each leaf's fuzzy signature once and
//! uses LSH (MinHash banding for text, Hamming pigeonhole banding for image
//! perceptual + ISCC) to narrow candidates. It is a **derived, discovery-grade
//! acceleration artifact** rebuildable byte-identically from
//! `manifest.parquet` + the CAS — NOT part of the signed trust chain. The exact
//! recheck and the signed inclusion proof stay in `attestrum-prove`, unchanged;
//! the index only changes which candidates get scored, never the score.
//!
//! See `docs/diagrams/index/sidecar-format.md` (on-disk format) and
//! `docs/diagrams/index/build-and-query.md` (build + query flows).

pub mod error;
pub mod format;
