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

**Not verified in any test:** tag reading order, screen-reader behavior, behavior on real books. The standalone veraPDF CLI could not be downloaded in the sandbox; round 2 (§8) ran veraPDF's PDF/UA-1 validation core from the jar bundled in `opendataloader-pdf` instead. "Better" below means richer structure, not certified conformance.

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

---

# Round 2 (2026-10-16): more GitHub projects and a veraPDF measurement

How candidates were found: web search restricted to github.com (the sandbox's GitHub search tools are scoped to one repo, and most non-GitHub sites are blocked), then each promising repo was cloned read-only. Search-result descriptions are the projects' own claims.

## 7. Additional projects

| Project | What it is | Checked how | License | Last commit |
|---|---|---|---|---|
| [opendataloader-project/opendataloader-pdf](https://github.com/opendataloader-project/opendataloader-pdf) | Java PDF parser with Python/Node wrappers. Free: layout analysis, auto-tag to **Tagged PDF**, header/footer filtering, hybrid mode (Docling backend, OCR, formulas, picture descriptions). **PDF/UA-1/2 export and a visual tag editor are enterprise add-ons.** Ships an agent skill (`skills/odl-pdf`). Built with PDF Association / Dual Lab (veraPDF developers). | **Installed v2.5.13 and ran it** (§8); README read | Apache-2.0 (veraPDF parts MPL-2.0) | 2026-10-15 |
| [pdf-accessibility-remediation/pdf-accessibility-remediation](https://github.com/pdf-accessibility-remediation/pdf-accessibility-remediation) | OCR (OCRmyPDF skip mode, pages with text untouched) → ODL auto-tag → Claude via OpenRouter returns a JSON **work order** (heading levels, alt text, artifacts, language, reading order) → deterministic apply → veraPDF (WCAG 2.2 machine profile) before and after → review.md classifying rules as fixed / still failing / newly failing. Cost estimate and confirmation before any paid call. Resume by run label. Stops PDF 2.0 and namespaced tag trees for a human. | README, docs list, intake code read | none found | 2026-10-13 |
| [MatiasAwdew/pdf-accessibility-remediator](https://github.com/MatiasAwdew/pdf-accessibility-remediator) | Python CLI + local Flask review app. Adobe AutoTag or built-in tagger; repairs headings, lists, tables, links, artifacts; Claude alt text with few-shot learning from reviewer corrections; font embedding; veraPDF validate-and-fix loop. Windows-oriented (desktop shortcut, Windows fonts, `%APPDATA%`). Lists PyMuPDF among its dependencies. Reports 3/3 held-out PDFs pass veraPDF (self-reported). | README read | none found | 2026-10-08 |
| [Mohakgarg5/KCP_PDF_REMEDIATION](https://github.com/Mohakgarg5/KCP_PDF_REMEDIATION) | Built for InDesign-exported Kellogg case PDFs (closest to book-style documents). Own content-stream tagger (~3.1k lines) + post-processor (~2.3k) + struct sanitizer; many dated regression tests; claims zero veraPDF failures on its cases (self-reported). | README, file sizes read | none found | 2026-10-01 |
| [anitasrinivasan/ada-pdf-compliance](https://github.com/anitasrinivasan/ada-pdf-compliance) | Claude Code plugin aimed at slide-deck exports: title, language, bookmarks from page titles, alt text, link descriptions; structure tree via Acrobat or a pikepdf path. | README read | MIT | 2026-03-31 |
| [mlibrary/analyze_pdf](https://github.com/mlibrary/analyze_pdf) | Analyzer (not a remediator): PDF/UA-1 and WCAG checks through veraPDF, JSON output. README says it was AI-generated and may contain errors. | README read | none found | 2026-04-15 |

## 8. veraPDF measurement (PDF/UA-1)

**Method.** `opendataloader-pdf`'s jar bundles veraPDF's validation core and its `PDFUA-1.xml` profile. A 40-line Java runner (`eval/poc/VerapdfCheck.java`) calls it. JDK 21 no longer ships JAXB, so three small jars (`jaxb-api-2.3.1`, `jaxb-core-2.3.0.1`, `jaxb-impl-2.3.0.1`) were added to the classpath. Same test files as §3: the flyer (tags stripped) and four one-page fixtures.

**Metadata patch used for the equal-footing runs** (`eval/poc/patch_ua.py`, ~15 lines of pikepdf): `MarkInfo/Marked = true`, `DisplayDocTitle = true`, `dc:title` in XMP and Info, `pdfuaid:part = 1`, `Lang` if missing, `/Tabs /S` on every page. This is a stand-in for SPEC S5's catalog fixes.

### 8.1 Flyer, as each tool left it (failed rules / failed assertions)

| Output | Rules | Assertions | What failed |
|---|---|---|---|
| Untagged input | 11 | 110 | Everything: no tags, no MarkInfo, no title, no PDF/UA id, content untagged (100+ assertions), fonts, widget |
| PDFix `AddTags` only | 4 | 4 | PDF/UA id; widget needs TU; fonts (7.21.4.1-1, 7.21.4.2-2) |
| PDFix batch (12 of 14 steps) | 3 | 3 | widget TU; fonts ×2 |
| accesspdf `fix` | 7 | 83 | 74 untagged content items; 4 figures without alt; widget not in Form tag; PDF/UA id; fonts |
| **ODL free tagger** | **7** | **7** | **All catalog-level** (MarkInfo missing, DisplayDocTitle, dc:title, PDF/UA id, Tabs) plus fonts. **No content-level failures.** |

### 8.2 Equal footing (same metadata patch applied to every output)

| File | ODL | PDFix `AddTags` | accesspdf |
|---|---|---|---|
| Flyer | **2 rules** (fonts) | 3 rules (widget TU, fonts) | 6 rules / 82 assertions (74 untagged content, widget nesting, 4 figures w/o alt, fonts) |
| headings | 1 rule (fonts) | 1 rule (fonts) | 1 rule (fonts) |
| multicolumn | 1 (fonts) | 1 (fonts) | 1 (fonts) |
| simple | 1 (fonts) | 1 (fonts) | 1 (fonts) |
| tables | 1 (fonts) | 1 (fonts) | 2 (fonts + 10 untagged content items) |

### 8.3 What this does and does not show

- **Supports "wrap ODL, own the PDF/UA layer":** with a trivial patch, the free ODL output had no content-level veraPDF failures on any of the five files. The paid "PDF/UA export" appears to cover the kind of document-level work our S5 would do. Whether it also does more is unknown.
- **Fonts are the remaining gap** on every file. The fixtures use non-embedded standard fonts (a worst case); the flyer is an InDesign export. Real books may differ. → SPEC promotes font repair into scope.
- **ODL's alt text is a placeholder:** all four figures got `image 1`…`image 4`, which passes veraPDF rule 7.3-1. PDFix copied caption text instead. Neither is a description. → SPEC check 13 and the judgment layer.
- accesspdf's tagging left many content items neither tagged nor artifacted on the designed flyer, but matched the others on simple fixtures.
- **Limits:** five small files; veraPDF checks only the machine-checkable subset; reading order, heading appropriateness and screen-reader behavior were not assessed; real books were not tested.

### 8.4 Additional findings that shaped the spec

11. The free ODL tier stops at Tagged PDF; vendor sells PDF/UA export. Keep the engine interface swappable.
12. Placeholder alt text passes veraPDF. Needs its own check.
13. Fonts were the last failing PDF/UA rules after metadata patching. Promote font repair into scope.
14. An independent project (pdf-accessibility-remediation) arrived at the same architecture: OCRmyPDF skip mode, ODL tagging, model-produced work order, deterministic apply, veraPDF gate, human review report. Cost confirmation, resume-by-label and "newly failing" regression classification are worth copying.

## 9. Leads found but not examined

- [PSPDFKit-labs/nutrient-skills](https://github.com/pspdfkit-labs/nutrient-skills): agent skills for a paid Nutrient API (20 auto-tagged pages/month free per its docs).
- [Community-Access/accessibility-agents](https://github.com/Community-Access/accessibility-agents): agent set with a PDF accessibility specialist (Matterhorn).
- [jshields-ca/pdf-accessibility](https://github.com/jshields-ca/pdf-accessibility) (Docubil): web app for WCAG analysis and safe auto-fixes.
- [iaminvester86/PDF-Remediation](https://github.com/iaminvester86/PDF-Remediation); [amytho/pdf-acc-toolset](https://github.com/amytho/pdf-acc-toolset) (C#/iText manual tag-tree editor, last push April 2024 per a listing).
- [mgifford/pdf-crawler](https://github.com/mgifford/pdf-crawler): finds PDFs on websites and flags accessibility issues (audit only).
- Emboss (`emboss-pdf`): *generates* PDF/UA-tagged PDFs from structured/LLM content and calls the veraPDF CLI; not a remediator, but a reference for writing tagged output.
- [latex3/tagging-project](https://github.com/latex3/tagging-project): tagged PDF from LaTeX at the source, relevant if any books originate in LaTeX.
- Commercial/hosted (not GitHub): Adobe PDF Accessibility Auto-Tag API, Nutrient, PDFix Actions, UCSD PDF Remediator, WMU ReadReady, ITHAKA's AWS pipeline (PDFix + veraPDF).
- Research: arXiv 2509.18965 (benchmark dataset and framework for PDF accessibility evaluation) and arXiv 2503.22216 (step-by-step remediation to improve tag accuracy, PAVE 2.0). The benchmark dataset may be usable for the Phase 0 corpus; check its license first.
- iTagPDF (CMU, CHI 2026): research system for academic PDFs; no evidence found that code is released.
