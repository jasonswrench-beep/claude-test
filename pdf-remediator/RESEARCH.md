# Research: PDF remediation tools compared

**Date:** 2026-10-15 · **Feeds:** [SPEC.md](SPEC.md)
**Purpose:** Evidence for designing our own tool. Not a purchase recommendation.

## 1. What was run vs. only read

| Tool | Version / commit date | Run by us? |
|---|---|---|
| PDFix SDK (`pdfix-sdk` 9.1.1 from PyPI, plus the user-supplied examples repo) | 9.1.1 | **Yes** |
| accesspdf (`accesspdf` 1.2.1 from PyPI; repo cloned) | 1.2.1, last commit 2026-03-01 | **Yes** |
| ASU PDF_Accessibility | last commit 2026-10-14 | No (needs AWS + Adobe credentials). Code and docs read. |
| CUNY AI Lab pdf-accessibility-app | last commit 2026-09-24 | No (needs Gemini key, Docker, veraPDF, Docling). Code and docs read. |
| nys-cdo poc-pdf-assessor | last commit 2026-07-30 | No. README and DECISIONS.md read. |
| ethan87w pdf-remediation | last commit 2026-10-14 | No. Code read. |
| Rafael-Calderon pdf-remediation-pipeline | last commit 2026-01-22 | No. Code read. |

Benchmarks quoted from CUNY's docs are the authors' own and were **not reproduced**.

**Not verified in any test:** veraPDF (download blocked in the sandbox), tag reading order, screen-reader behavior, behavior on real books. "Better" below means richer structure, not certified conformance.

## 2. Landscape

| Tool | What it is | Writes content tags? | Alt text | Validation | Needs | License |
|---|---|---|---|---|---|---|
| PDFix SDK | Commercial native SDK with Python bindings; batch commands and low-level tag API. Examples repo = 26 API samples. | Yes (heuristic) | Copies nearby caption text | Preflight only | pip + ~60MB binary | Proprietary (pdfix.net/terms) |
| ASU PDF_Accessibility | AWS CDK: Adobe Autotag/Extract + Bedrock model for alt/title/link text; Adobe checker before and after; Step Functions chunking | Yes (Adobe cloud) | LLM | Adobe checker | AWS, Adobe API contract/trial | MIT |
| CUNY app | Classify → OCR → Docling → Gemini decisions → deterministic pikepdf tagger → veraPDF + fidelity gate → release-ready vs manual | Yes (own tagger, ~5.6k lines) | LLM + decorative detection | veraPDF + text-drift/reading-order/table fidelity | Gemini key, Docker, veraPDF, Tesseract, Docling | **None** |
| accesspdf | Local CLI + web UI + TUI, sidecar YAML alt text, optional Ollama/Gemini/Claude/OpenAI | Yes (weaker on designed docs) | Sidecar, human-approved | Own checker + contrast | pip, no key | Apache-2.0 |
| nys-cdo assessor | Triage: effort hours × reach → remediate / retire / replace-with-HTML; archive-exemption logic; audit trail | No | No | Detects "shell tagging" | Node 22, SQLite | **None** |
| ethan87w pdfremediate | Sets lang, title, Marked flag, **empty** StructTreeRoot; Gemini suggestions go to a worklist; ocrmypdf/Gemini OCR; hash-keyed cache | **No** | Worklist only | None | pikepdf, PyMuPDF | **None** |
| Rafael-Calderon pipeline | Metadata fixer for Florida tax forms; optional Acrobat Pro automation | **No** (empty Document element) | No | None | Node, pdf-lib | MIT |

URLs: [PDFix examples](https://github.com/pdfix/pdfix-sdk-pypi-package) (package home) · [ASU](https://github.com/ASUCICREPO/PDF_Accessibility) · [CUNY](https://github.com/CUNY-AI-Lab/pdf-accessibility-app) · [accesspdf](https://github.com/laurenaulet/accesspdf) · [NYS](https://github.com/nys-cdo/poc-pdf-assessor) · [ethan87w](https://github.com/ethan87w/pdf-remediation) · [Rafael-Calderon](https://github.com/Rafael-Calderon/pdf-remediation-pipeline)

## 3. Hands-on results

### 3.1 Method

- Input: PDFix's `resources/test.pdf` (one-page designed flyer: banner, 3 icons with captions, 3 tables, a bullet list, text). The file as supplied is already tagged (InDesign export with custom tags `Story`, `NormalParagraphStyle`, `Article`; 4 figures, none with alt text; no title).
- For a fair comparison, tags were stripped (`doc.RemoveTags()`) to make `test_untagged.pdf`, then both tools run on it.
- Inspection with `pikepdf` (tag counts, Lang, Title, MarkInfo, XMP).
- Second test: four one-page fixtures from accesspdf's own `tests/corpus/` (headings, tables, multicolumn, simple).

### 3.2 Flyer, untagged input

| Result | PDFix `AddTags` | accesspdf `fix` |
|---|---|---|
| Headings | H1 ×1, H2 ×1 | none |
| Paragraphs | 36 `P` | 85 flat `P` |
| Lists | `L` with 4 `LI` (Lbl + LBody) | none |
| Tables | 4 | 1 (page has 3 visible boxes) |
| Figures | 3, all with alt | 4, none with alt |
| Title set | "The PDFix SDK" | "The PDFix SDK" |
| Other | `Form` ×1; `NonStruct` root wrapper | — |

- PDFix alt text was the **caption text beneath each icon** ("Standard PDF Features"), not a description of the image. It dropped the decorative banner (correct).
- accesspdf's `check` flagged **white text on green as a contrast failure**; PDFix has no contrast check.

### 3.3 Flyer, already-tagged input

accesspdf reported 3 changes (the output shows the title was set; its code also ensures page tab order, which I did not verify in the output) and left the custom tags and the 4 figures without alt text untouched. Re-check still reported "4 images missing alt text" and the contrast issue.

### 3.4 accesspdf's own fixtures (one page each)

Roughly even with PDFix on headings, tables and paragraphs. Neither replaced the junk title "(anonymous)". So accesspdf holds up on clean synthetic files and is weaker on a real designed document. **One flyer is not a benchmark.**

### 3.5 PDFix `make_accessible` batch (from the examples' `resources/make-accessible.json`)

| Step | Result unlicensed |
|---|---|
| clear_structure, flatten_xobject, embed_font | ok |
| **add_missing_unicode** | **FAIL: "Invalid action object"** |
| **preflight** | **FAIL: "Invalid action object"** |
| add_tags, set_language, set_title, set_display_doc_title, set_pdf_ua_standard, set_suspect_value, fix_oc_name, fix_media_clip_keys, create_bookmarks | ok |

- Whether the two failures are licensing or environment is **unknown**.
- Run as one batch, the failure left the in-memory document with its tags stripped (`clear_structure` had already run). A script that saved after a failed batch would have shipped an untagged file. Basis for SPEC principle 5 (atomic output).
- Stepwise run produced Title, Lang, `DisplayDocTitle`, XMP `pdfuaid:part = 1`, bookmarks, and `Suspects = false`.
- Unlicensed, an 8-page copy of the flyer was tagged on every page with no visible watermark. Terms not read; treat commercial use as unverified.

## 4. Findings that shaped the spec

1. **Two of the seven tools write an empty or near-empty tag tree** (ethan87w: empty `StructTreeRoot`; Rafael-Calderon: a `Document` element with no children). Both set `Marked=true` (Rafael also stamps PDF/UA identifiers), so shallow checks pass while a screen reader gets nothing. The existing skill documents the same failure mode ("phantom tree") from its own engine. → Phantom-tree hard gate.
2. **The content-stream tag writer is the hard part.** CUNY needed ~5.6k lines for tagging and ~2.3k for fidelity checking. ethan87w's own docstring calls it "a large undertaking" and skips it. → Wrap first.
3. **Only CUNY verifies that content survived** (text drift, reading order, table coverage). → Fidelity gate.
4. **Alt text from nearby captions is not a description.** LLM alt text needs human review. → Sidecar + review packet.
5. **"Already tagged" is not "well tagged".** → Tag-quality score in classification.
6. **A failed pipeline step can leave a damaged document.** → Atomic writes.
7. **Triage matters at scale** (NYS): estimate effort, decide remediate/retire/replace before spending effort. → Triage summary in batch runs.
8. **Cheap LLM design:** small, per-page batched, hash-cached requests (ethan87w, CUNY). → §5 of the spec.
9. **Provider abstraction and sidecar approval** (accesspdf) map directly to bring-your-own-key and the human check.
10. **Licensing constrains reuse:** CUNY, NYS and ethan87w have no license file (all rights reserved by default); PDFix is proprietary.

## 5. Notes from the existing `pdf-remediator` skill

These are the user's current pipeline and are the baseline the new tool should beat:

- Classes: `tagged`, `phantom_tree`, `born_digital_untagged`, `scanned` (by StructTreeRoot, BDC count, first-page text length).
- Engine: `opendataloader-pdf` (free, local, Apache-2.0 per the skill, Java). OCR: `ocrmypdf` (standard, then `--force-ocr`).
- Catalog patch: `MarkInfo`, `Lang`, `Title` (placeholder titles overwritten).
- Residual issues it cannot fix: figure alt text, table headers/scope, form tooltips, complex reading order, math, bookmarks, heading hierarchy in complex layouts, contrast, mixed languages.
- Ally bands (per the skill): Perfect 100%, High 67–99%, Medium 34–66%, Low 1–33%, None 0%.
- Ally checks listed by the skill: tagged, language, title, headings, alt text, table headers, no image-only pages, selectable text, contrast, reading order. (Source is the skill's notes; verify against Blackboard documentation.)
- BDC-count bands as a rough quality signal: ≥200 strong, 50–199 moderate, 1–49 weak, 0 failed.
- Gaps the new tool closes: alt-text generation, table and contrast handling, an automatic validation gate with fidelity checks, cross-platform dependency check, per-page classification, review packet.
- The skill's scripts say they were adapted from "pdf-remediation-tool by T. Whetsell": confirm reuse terms before copying.

## 6. Reproduction

```bash
python3 -m venv venv && ./venv/bin/pip install pdfix-sdk==9.1.1 pikepdf pypdf
python3 -m venv venv_ap && ./venv_ap/bin/pip install accesspdf
# strip tags (PDFix): doc.RemoveTags(); doc.Save(...)
# tag (PDFix):       tmpl = doc.GetTemplate(); tmpl.AddPage(i) for all pages; tmpl.Update(); doc.AddTags(PdfTagsParams())
# tag (accesspdf):   accesspdf check x.pdf ; accesspdf fix x.pdf -o x_fixed.pdf
```

Phase 0 should rebuild this as a script in `eval/` with veraPDF included, and extend it to book-like inputs.
