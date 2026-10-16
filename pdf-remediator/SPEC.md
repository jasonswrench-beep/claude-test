# PDF Remediator — Product & Engineering Spec

**Working title:** PDF Remediator (name TBD)
**Status:** Draft v0.2 — 2026-10-16 (see Changelog at the end)
**Origin:** Tool comparison session (evidence in [RESEARCH.md](RESEARCH.md))
**Owner:** Jason Wrench

---

## 0. Resume here (for desktop Claude Code)

### What is decided

| Topic | Decision |
|---|---|
| Form factor | A program on a person's hard drive. CLI first, local web UI for review later. No hosted service in v1. |
| Privacy model | Local by default. Judgment calls (alt text etc.) use a **bring-your-own** model: the user supplies an API key or points at a local model. |
| Document mix | Mostly PDFs and **books** (long, born-digital). Scans and slides are possible but rarer. OCR step required for files with no text layer. |
| Operation | Runs unattended. A human always checks the result when it is uploaded for accessibility. |
| Pass bar | **Both** Canvas Ally *and* PDF/UA-1 via veraPDF, plus WCAG 2.1 AA checks. See §2. |
| Engine strategy | **Wrap first, build later.** Use an existing tagger, own everything around it. See §4 and §11. |

### What is still open (confirm early)

1. Ally threshold: **High minimum, Perfect target** assumed (matches the existing `pdf-remediator` skill). Confirm.
2. AI drafts: **apply to the output and flag for review** (assumed, because runs are unattended) vs hold until approved. See §5.4.
3. Target platforms: macOS, Windows, Linux? Assumed all three.
4. Product name.

### First steps on desktop

```bash
git clone https://github.com/jasonswrench-beep/claude-test
cd claude-test
git checkout claude/epic-shannon-q23swq
# read: pdf-remediator/SPEC.md and pdf-remediator/RESEARCH.md
```

Install locally before Phase 0: Python 3.11+, Java 11+, [veraPDF](https://verapdf.org/), Tesseract, Ghostscript, `ocrmypdf`, `opendataloader-pdf`. (Install the real veraPDF CLI. It was unreachable from the cloud sandbox where this was drafted, so the Phase 0 evidence so far used the veraPDF validation core bundled inside the `opendataloader-pdf` jar, via the small runner in `pdf-remediator/eval/poc/`. See RESEARCH.md §8.)

### Prompt to paste into the first desktop session

> Read `pdf-remediator/SPEC.md` and `pdf-remediator/RESEARCH.md`. We are starting **Phase 0** (§11). Verify that Java, veraPDF, ocrmypdf and Tesseract are installed. Then build the test corpus and scoring harness under `pdf-remediator/eval/` and run the engine bake-off described in §11. Report results and a recommendation before starting Phase 1. Do not copy code from repos listed as "no license" in RESEARCH.md.

---

## 1. Purpose and scope

### 1.1 Goal

Take a PDF (typically a course reading, a chapter, or a whole book) and produce an **accessibility candidate**: a PDF that passes automated PDF/UA-1 validation and is expected to score well in Canvas Ally, plus a **review packet** that tells a human exactly what to check.

### 1.2 In scope (v1)

- Born-digital PDFs, including long books (hundreds of pages).
- Scanned or partly scanned PDFs, via an OCR step.
- Tagging, reading order, headings, lists, tables, links, figures and alt text, artifacts (running headers, footers, page numbers), bookmarks, document metadata.
- Validation against PDF/UA-1 (veraPDF) and local proxies for Ally's checks.
- Batch processing of a folder.

### 1.3 Out of scope (v1)

- Certifying compliance. The tool never says "compliant" (§2.4).
- Editing the visual content of a PDF.
- Fillable-form remediation, slide decks, math (MathML). These are **detected and flagged**, not fixed.
- Uploading to any LMS. That is always the user's action.
- A hosted or multi-user service.

### 1.4 Users

Faculty, staff, and authors preparing documents for an LMS. Assume comfortable with installing software, not with the command line, so a local UI follows the CLI (Phase 3).

---

## 2. Pass bar (definition of done)

Three gates. A file reaches the top status only when all three are satisfied.

### 2.1 Gate 1 — PDF/UA-1 (automated, veraPDF)

- Run `verapdf --flavour ua1 <file>` and parse the report. (Confirm output-format flags with `verapdf --help` on the installed version.)
- **Pass:** zero failed rules, or every remaining failure is on a documented waiver list with a reason.
- Caveat to state in every report: veraPDF checks only the machine-checkable part of PDF/UA. Alt-text quality, reading-order correctness and heading appropriateness need a human (see the Matterhorn Protocol).
- **veraPDF accepts placeholder alt text.** The free `opendataloader-pdf` tagger writes `image 1`, `image 2`, … as `/Alt`, which satisfies the machine rule (7.3-1) while telling a screen-reader user nothing. Gate 2 check 13 exists for this reason.

### 2.2 Gate 2 — Ally and WCAG proxy (automated, local)

Ally cannot be run locally. The tool runs local checks that mirror what Ally is understood to check (per the existing skill's `lms-targets.md`; verify against Blackboard documentation):

| # | Check | How |
|---|---|---|
| 1 | Document is *genuinely* tagged: structure tree **and** marked content linked to it | StructTreeRoot present, BDC/MCID linkage resolves. A tree with no linked content is a failure ("phantom tree"). |
| 2 | Language set | `/Lang` on catalog, valid BCP-47 |
| 3 | Real title | Not a placeholder (`untitled`, `Microsoft Word - …`, `(anonymous)`); `DisplayDocTitle` true |
| 4 | Headings exist, no skipped levels | Walk tags |
| 5 | Every figure has alt text or is an artifact | Walk tags |
| 6 | Tables have header cells with scope | Walk tags |
| 7 | No image-only pages without a text layer | Per-page text check, post-OCR |
| 8 | Text is selectable and maps to Unicode | Font/ToUnicode checks |
| 9 | Contrast of text against its background | Render and sample (accesspdf has a reference approach) |
| 10 | Reading order plausible | Compare tag order to geometric order; flag disagreements |
| 11 | Bookmarks present on long documents | Threshold configurable (default 20 pages) |
| 12 | Links tagged and have meaningful text | Link tags with OBJR; flag "click here" |
| 13 | Alt text is not a placeholder | Reject `image N`, file names, bare "Figure"/"Image", empty, or very short/very long strings; route to the judgment layer |

### 2.3 Gate 3 — Human verification (recorded)

The user uploads to Canvas, reads the Ally score, and runs a quick spot check (screen reader on a sample if available). The tool provides a **ledger** (CSV/JSON) to record: file, date, tool version, Gate 1 result, Ally score, reviewer, notes. Target: **Ally High (67–99%) minimum, Perfect (100%) as the goal.**

### 2.4 Status vocabulary

| Status | Meaning |
|---|---|
| `failed` | Pipeline error, or output is a phantom tree. Nothing shipped. |
| `needs-manual` | Gates 1–2 not met. Report lists specific blockers and fix paths. |
| `candidate` | Gates 1–2 met. Awaiting human check. |
| `verified` | Human recorded an Ally score meeting the threshold in the ledger. |

The words "compliant" and "remediated" are never used by the tool for an unverified file.

### 2.5 Fidelity gate (runs alongside Gates 1–2)

Remediation must not change what the document says or looks like:

- Extracted text before vs after matches within a configurable drift threshold (OCR pages excluded from the "before" side).
- Page count unchanged.
- Rendered pages before vs after are visually identical for non-OCR pages (pixel or perceptual diff).
- Reading order check (see Gate 2 #10).

---

## 3. Principles (non-negotiable)

1. **Never modify the original.** All work happens on copies; outputs go to a separate path.
2. **The model never writes PDF objects.** LLMs return structured decisions (JSON); deterministic code writes the PDF.
3. **Never ship a phantom tree.** "Tagged" means content-linked. Hard gate.
4. **Nothing leaves the machine unless configured, and the user can see what left.** Every run prints a data-sent summary. `--local-only` forbids network calls.
5. **Atomic output.** Write to a temp file, validate, then move into place. A failure mid-pipeline never leaves a partially processed file in the output folder. (Lesson from the PDFix test, where a failed batch step had already stripped the tags.)
6. **Idempotent and resumable.** Same input and same decisions give the same output. Long jobs resume from cache.
7. **Cache everything judgment-related** by hash of exact input (content-hash IDs for images).
8. **Per-file isolation.** One bad file never aborts a batch. One failed processor adds a warning, not a crash.
9. **Honest reporting.** Always list what was done, what was not, and what a human must check.

---

## 4. Architecture

```
Input PDF (copy)
  │
  ├─ S0  Doctor (once per install/run)      dependencies present?
  ├─ S1  Intake & classify (per page)       tagged | phantom | untagged | scanned | mixed
  ├─ S2  Repair (if parse fails)            qpdf
  ├─ S3  OCR (pages that need it)           ocrmypdf --skip-text
  ├─ S4  Tag engine (pluggable)             produces/repairs tag tree
  ├─ S5  Enrich & normalize (pikepdf)       artifacts, headings, tables, links, bookmarks, metadata
  ├─ S6  Judgment layer (BYO model)         alt text, table headers, headings, title  → sidecar
  ├─ S7  Write (deterministic, atomic)
  ├─ S8  Validate                           veraPDF + local checks + fidelity
  └─ S9  Report & review packet             HTML + JSON + sidecar; ledger entry stub
```

### 4.1 S0 — Doctor

`doctor` command checks Java ≥ 11, veraPDF, Tesseract (and language packs), Ghostscript, ocrmypdf, engine packages, and optional provider keys. Cross-platform Python, **not** a bash script (the existing skill's `bootstrap.sh` does not run on Windows). Fails fast with install instructions. The existing skill notes `opendataloader-pdf` fails silently without Java, so check it explicitly.

### 4.2 S1 — Intake and classify

Per **page**, not just per file. Books often have a scanned insert or a tagged-badly front section. Extends the existing skill's four classes:

| Class | Rule |
|---|---|
| `tagged` | StructTreeRoot present **and** marked content linked |
| `phantom_tree` | StructTreeRoot present, no linked content → treat as untagged |
| `born_digital_untagged` | Text layer, no tree |
| `scanned` | No text layer on that page |
| `mixed` | File has both digital and scanned pages |
| `error` | Parse failure → S2 |

Also record: page count, file size, languages detected, presence of forms, annotations, embedded fonts, slide-like pages (landscape, sparse), and an **already-tagged quality score** (tag types used, custom roles via RoleMap, heading count, figures without alt). "Tagged" is not "well tagged": in testing, an already-tagged InDesign export had custom tag names and no alt text, and one tool left it untouched.

Classification is cheap and its output drives routing. It also feeds a one-line triage summary.

### 4.3 S3 — OCR

- Tool: `ocrmypdf` (Tesseract). Free, local.
- Use `--skip-text` so digital pages are untouched on mixed files. `--force-ocr` (rasterizes existing text) is a last resort and must be explicit in the report.
- Detect language per document and pass the matching Tesseract language pack; doctor warns if missing.
- OCR text is **unverified**. Any page that went through OCR is listed in the review packet.
- Never fabricate a text layer silently. If OCR is unavailable, flag and stop that file.

### 4.4 S4 — Tag engine (the wrap-first decision)

Define an interface so engines are swappable:

```
class TagEngine(Protocol):
    name: str
    def tag(self, pdf_in: Path, pdf_out: Path, opts) -> EngineResult
```

**Mode A (v1): engine produces a tagged PDF; our S5 post-processes it.**

| Engine | Role | Notes |
|---|---|---|
| `opendataloader-pdf` | Default | Free (Apache-2.0), local, Java; already used by the existing skill. **Its README says the free tier outputs Tagged PDF only; PDF/UA-1/2 export is a paid enterprise add-on**, so our S5 must supply the PDF/UA-level fixes. Measured: on the test flyer, ODL plus a 15-line metadata patch left only font failures under veraPDF (RESEARCH.md §8). It writes placeholder alt text (`image N`). Quality on **books untested**. |
| PDFix SDK | Optional plug-in | Proprietary. Strongest heuristic tagger in testing (headings, lists, tables, forms) but cannot be bundled. Usable by anyone with their own license. |
| Adobe PDF Services | Optional plug-in | Cloud API. Violates local-first and needs data-governance sign-off (FERPA). Opt-in only. |
| accesspdf tagger | Candidate | Apache-2.0, local. Weaker on a real designed document in testing. |

The **bake-off in Phase 0 chooses the default** (§11). Engine fallback ladder carried over from the existing skill: engine → OCR → force-OCR → (manual Acrobat) → optional cloud engine.

**Mode B (later): our own writer from a document model.** Extract structure with a layout engine (Docling or similar) into a `DocumentModel` (blocks with type, bbox, page, reading order, confidence), then write tags ourselves with pikepdf. This is what the CUNY app does (about 8k lines for tagging and fidelity checking). Only build it if the bake-off shows Mode A engines fail on the real document mix.

### 4.5 S5 — Enrich and normalize (pikepdf)

Deterministic repairs on whatever the engine produced:

- **Artifacts:** mark running headers, footers, and page numbers as `/Artifact` (detect text repeated at a similar position on many pages). Needs content-stream edits *and* structure-tree edits. **This is the hardest S5 item and where "wrap" vs "build" gets blurry;** spike it first.
- **Headings:** normalize hierarchy (no skipped levels; sensible H1); map custom roles via RoleMap to standard roles.
- **Tables:** header cells `TH` with `Scope` (Row/Column); `Headers`/`ID` for complex tables; flag cross-page tables.
- **Lists:** `L`/`LI`/`Lbl`/`LBody` correct nesting.
- **Links:** `Link` tags with OBJR and Contents/alt; flag non-descriptive text.
- **Footnotes/endnotes:** `Note` tags where detectable.
- **Bookmarks:** build from headings (threshold for long docs). Optional TOC/TOCI tagging.
- **Metadata:** `MarkInfo/Marked`, `Lang`, `dc:title` in XMP plus Info, `DisplayDocTitle`, XMP `pdfuaid:part = 1`, page `/Tabs /S`.
- **Fonts:** detect unembedded fonts, incomplete CIDSet and missing ToUnicode, and attempt repair (embed or substitute, subset, widths, ToUnicode). **Promoted from out-of-scope:** after metadata patching, font rules (veraPDF 7.21.4.x) were the *only* remaining PDF/UA-1 failure on every ODL and PDFix output in the proof of concept. How common this is in real books is unknown (the fixtures used non-embedded standard fonts); measure it in Phase 0.

Each processor is independent, runs in try/except, and reports `changes_made` and `warnings` (accesspdf pattern).

### 4.6 S6 — Judgment layer

See §5.

### 4.7 S7 — Write

Deterministic. Applies engine output, S5 repairs and approved/accepted S6 decisions to a temp file, then S8 validates before the file is moved into the output folder.

### 4.8 S8 — Validate

veraPDF (Gate 1), local checks (Gate 2), fidelity (§2.5). Produces a status per §2.4. If Gate 1 or 2 fails because of something a fixer could handle, run the fixer once and re-validate; never loop indefinitely.

### 4.9 S9 — Report and review packet

Per file: `*.accessible.pdf`, `*.report.json`, `*.review.html`, `*.alttext.yaml` (sidecar). The HTML packet lists, with page thumbnails: every AI-generated alt text, every OCR'd page, low-confidence decisions, residual issues with fix paths, and Gate results. Batch runs add a summary table and a triage view.

---

## 5. Judgment layer (bring-your-own model)

### 5.1 Provider interface

One protocol, one file per provider; no provider-specific code elsewhere.

- Providers: Anthropic, OpenAI, Gemini, and any **local OpenAI-compatible endpoint (Ollama etc.)**. The local endpoint is the "local path": same interface, no network.
- Optional local alt-text path: ODL's hybrid mode (`--enrich-picture-description`) produces picture descriptions locally. Untested; compare against Ollama vision models in Phase 2.
- Keys: environment variable, or OS keychain (`keyring`). Never written to config files, reports, logs, or the repo. Web UI (later) takes the key per request.
- `--no-ai`: everything deterministic; alt text stays `needs_review` and is never invented.

### 5.2 Tasks

| Task | Input | Output |
|---|---|---|
| T1 Alt text | Downscaled image crop, nearby text/caption, doc title | `{alt, decorative, kind, confidence}`; charts get short alt plus a long-description flag |
| T2 Table headers | Cell-text grid | Header rows/cols, simple vs complex |
| T3 Heading levels | Candidate lines with size/position/context | Level or "not a heading" |
| T4 Title | First-page text | Title string |
| T5 Reading order | Only low-confidence regions | Ordered block ids |
| T6 Link text | Link context | Suggested description (advisory) |

Requests are small, **batched per page**, and **cached by input hash**. Systems that batched this way reported large reductions in request count (CUNY: 89 → 5 requests on one form, self-reported).

### 5.3 Privacy

- Only image crops and short text snippets are sent. Never the whole document unless the user configures a provider that requires it.
- Every run prints and records a **data-sent summary** (what, how many items, which provider).
- `--local-only` blocks all network access except to localhost.
- Before any paid model call, print the estimated request count and cost and ask for confirmation (`--yes` skips). Pattern taken from pdf-accessibility-remediation (RESEARCH.md §7).
- Cloud providers: README text and doctor output remind users to get institutional sign-off for FERPA-protected material.

### 5.4 Review policy

- Default (`ai_apply = draft`): AI results are applied to the output so the file is ready to upload, and each is marked `needs_review` in the sidecar and review packet. This fits the stated workflow (unattended run, human checks at upload).
- Alternative (`ai_apply = hold`): AI results are written only to the sidecar; a human approves, then a re-run applies them.
- Human edits to the sidecar always win and survive re-runs (IDs are content hashes).
- Decorative images are marked as artifacts, not given empty alt.

---

## 6. Books and long documents

Treat as the primary case, not an edge case.

| Concern | Requirement |
|---|---|
| Running headers/footers/page numbers | Detect and artifact (§4.5) |
| Heading hierarchy | Chapter → section → subsection normalized; no skipped levels |
| Bookmarks and TOC | Generate from headings; tag TOC entries as links where possible |
| Footnotes | `Note` tags, linked where detectable |
| Cross-page tables | Flag; repeat header semantics where feasible |
| Multi-column | Reading order check and flag |
| Index, bibliography | Flag; lists and links tagged |
| Scale | Process in page chunks, resumable; cache per chunk; progress and ETA. Existing systems cap pages (CUNY default 300) or split and merge (ASU); we need an explicit policy |
| Performance | Record pages/second per stage in the eval harness; set budgets after Phase 0 |
| Page labels | Preserve or set logical page labels |

**Known gap:** none of the compared tools was tested by us on a real book. Building the book fixtures is the first Phase 0 task.

---

## 7. Dependencies and packaging

| Need | Choice | Note |
|---|---|---|
| Language | Python 3.11+ | |
| PDF editing | `pikepdf` | |
| Text/layout reading | `pdfminer.six` and/or `pypdfium2` | Prefer permissive licenses |
| Render | `pdftoppm` (Poppler) or `pypdfium2` | For thumbnails, contrast, render diff |
| OCR | `ocrmypdf` + Tesseract + Ghostscript | |
| Validation | veraPDF + Java ≥ 11 | |
| Default engine | `opendataloader-pdf` (Java) | Pending bake-off |
| CLI | `typer` | |
| Config | TOML | |
| Secrets | `keyring` | |
| Local UI (Phase 3) | FastAPI + simple HTML | |

**License caution:** PyMuPDF is AGPL (commercial license available); avoid it in a distributable tool unless the licensing question is settled. PDFix is proprietary and cannot be bundled. Confirm the terms of anything added.

**Packaging path:** dev install (`uv`/`pipx`) in Phases 1–2 → single-command installer or bundle that checks or fetches Java, veraPDF and Tesseract in Phase 3. The install burden (Java + Tesseract + Ghostscript) is a real adoption risk for non-technical users.

---

## 8. Interfaces

### 8.1 CLI

```
remediator doctor                       # check dependencies
remediator check <pdf|dir>              # classify + validate only; read-only
remediator fix <pdf|dir> -o <outdir>    # full pipeline
remediator review <outdir>              # open review packet / local UI
remediator ledger add|list|export       # record Ally scores and sign-offs
remediator engines                      # list engines and providers
```

Common flags: `--engine`, `--provider`, `--no-ai`, `--local-only`, `--lang`, `--ai-apply draft|hold`, `--resume`, `-r` (recurse), `--dry-run`.

### 8.2 Config (`remediator.toml`)

```toml
[defaults]
engine = "opendataloader"
lang = "en-US"
ai_apply = "draft"          # draft | hold
bookmark_min_pages = 20
ally_target = "high"        # high | perfect

[provider]
name = "anthropic"          # anthropic | openai | gemini | local
base_url = ""               # for local OpenAI-compatible endpoints
model = ""
# key comes from env var or OS keychain, never from this file

[privacy]
local_only = false
```

### 8.3 Output layout

```
<outdir>/
  <name>.accessible.pdf
  <name>.report.json
  <name>.review.html
  <name>.alttext.yaml
  ledger.csv
  cache/
```

---

## 9. Data formats

### 9.1 Sidecar (`*.alttext.yaml`)

```yaml
document: book.pdf
generated: 2026-10-15T00:00:00Z
images:
- id: img_37044c            # hash of image bytes; stable across re-runs
  page: 12
  bbox: [72, 400, 300, 600]
  kind: chart               # photo | chart | diagram | equation | decorative | other
  ai_draft: "Bar chart of quarterly revenue, 2023–2025."
  alt_text: "Bar chart of quarterly revenue. Q1 2025 is highest at $4.2M."
  long_description: ""
  confidence: 0.82
  status: needs_review      # needs_review | approved | decorative
  reviewer: ""
```

### 9.2 Report (`*.report.json`) — minimum fields

`tool_version`, `engine`, `provider` (name only), `classification` (per page), `ocr_pages`, `processors[]` (`name`, `changes`, `warnings`, `error`), `gate1` (veraPDF failed rules), `gate2` (checks with pass/fail), `fidelity`, `status`, `data_sent` summary, `residual_issues[]` with fix paths.

### 9.3 Ledger (`ledger.csv`)

`file, date, tool_version, status, verapdf_failed_rules, ally_score, ally_band, reviewer, notes`.

---

## 10. Testing and evaluation

### 10.1 Corpus (`pdf-remediator/eval/corpus/`)

Tiers, each with a ground-truth description:

1. Simple text; headings; tables; multi-column (accesspdf's fixtures are a starting point, licensed Apache-2.0).
2. **Book-like:** 200+ pages, chapters, running headers/footers, footnotes, a cross-page table, figures with captions, TOC, index.
3. Already tagged but bad: phantom tree; custom roles with no alt; "everything is `P`".
4. Scanned; mixed digital + scanned.
5. Slides; forms (detect-and-flag only).
6. The designed flyer from the comparison session (icons with captions, tables, list, banner).

Use only files whose redistribution is allowed; keep third-party or private documents out of the repo.

### 10.2 Scoring harness

For every file and engine: veraPDF failed-rule count by rule; Gate 2 pass/fail per check; tag richness (headings, lists, tables with TH, figures with alt, artifacts); text fidelity; render diff; runtime and pages/second; (when AI used) request count and cost. Output a comparison table; commit results.

### 10.3 Regression and release checks

- Corpus run in CI (or a local script) on every change to S4–S8; fail on new phantom trees.
- Manual per release: upload a sample set to Canvas, record Ally scores in the ledger; screen-reader spot check on at least one book chapter.

---

## 11. Phased plan

### Phase 0 — Spike and bake-off

**Goal:** choose the default engine with evidence, and de-risk the hard parts.

1. Install the stack on the desktop (§0). Run `veraPDF` on one file to confirm it works.
2. Build the corpus (§10.1), especially the book-like fixtures.
3. Build the scoring harness (§10.2).
4. Run candidate engines: `opendataloader-pdf`, PDFix SDK (pip `pdfix-sdk`; read its terms first), accesspdf's tagger. Adobe only if credentials and sign-off exist. **ODL, PDFix and accesspdf have already been measured on one flyer and four one-page fixtures (RESEARCH.md §8); extend that to the book fixtures.** Reuse `eval/poc/` (veraPDF runner, metadata patch, inspector).
5. Spike **artifacting running headers/footers** on a tagged PDF with pikepdf; estimate effort.
6. Write results to `pdf-remediator/eval/RESULTS.md` with a recommendation: default engine, and whether Mode B is needed.

**Exit:** a chosen default engine, baseline numbers, and a go/no-go on the artifact spike.

### Phase 1 — Core CLI (no AI)

`doctor`, classify (per page), repair, OCR routing, engine wrapper, S5 processors, veraPDF gate, fidelity checks, atomic write, report JSON, `check` and `fix` commands, batch with resume.

**Exit:** every corpus file ends as `candidate` or `needs-manual` with specific reasons; zero phantom trees shipped; originals untouched; results table committed.

### Phase 2 — Judgment layer

Provider interface (Anthropic, OpenAI, Gemini, local endpoint), T1–T4 (T5–T6 optional), cache, sidecar, data-sent summary, `--no-ai` and `--local-only`, review packet HTML.

**Exit:** alt-text coverage on corpus figures; no network calls under `--local-only` (tested); keys never appear in outputs (tested).

### Phase 3 — Usability

Local review UI, ledger commands, batch triage view, installer or bundle, docs for non-technical users, macOS/Windows verification.

### Phase 4 — Hardening and extension

Scale on large books; Mode B writer if Phase 0/1 data justify it; optional PDFix/Adobe plug-ins; slide and form handling; font repair.

---

## 12. Risks and open questions

| Risk | Mitigation |
|---|---|
| Engine quality on books is unknown | Phase 0 bake-off on real book fixtures before committing |
| Free ODL tier stops at Tagged PDF; the vendor sells PDF/UA export | Own the S5 PDF/UA layer; keep the engine interface so the engine can be swapped |
| Placeholder alt text passes veraPDF | Gate 2 check 13; judgment layer replaces placeholders; review packet lists them |
| Artifacting inside an already-tagged PDF is hard | Spike in Phase 0; budget for a content-stream rewrite |
| Ally cannot be run locally; proxy may diverge | Ledger of real Ally scores; tune proxy checks against it |
| veraPDF pass ≠ accessible | Human gate; report states the limit |
| Install burden (Java, Tesseract, Ghostscript) | `doctor`; bundle in Phase 3 |
| Licensing | Do not copy CUNY/NYS/ethan87w code (no license). PDFix proprietary. PyMuPDF AGPL. The existing skill's scripts say "adapted from pdf-remediation-tool by T. Whetsell": confirm reuse terms before copying |
| Data governance | Local default; cloud only by explicit config; FERPA reminder |
| AI-applied alt text may be wrong | Always flagged `needs_review`; `ai_apply = hold` available; charts get long-description flag |
| OCR errors propagate into tags | List OCR'd pages in review packet |

### Open decisions to confirm

1. Ally threshold (High vs Perfect) and whether Perfect should block `candidate`.
2. `ai_apply` default (draft vs hold).
3. Platforms to support in v1.
4. Whether to record Ally scores by hand only, or also support importing Ally reports if available.
5. Name.

---

## 13. Reference implementations (license-aware)

| Project | Learn from | License | Reuse |
|---|---|---|---|
| CUNY-AI-Lab/pdf-accessibility-app | Pipeline spine; deterministic tagger; fidelity checks; veraPDF gating; release gate | none | Ideas only |
| laurenaulet/accesspdf | Sidecar workflow; provider abstraction; processors; contrast check; web UI | Apache-2.0 | May adapt with attribution |
| ASUCICREPO/PDF_Accessibility | Chunked split/process/merge at scale; pre/post accessibility check | MIT | May adapt with attribution |
| PDFix SDK + examples | API reference for tag editing; benchmark for heuristic tagging | Proprietary | Reference only |
| nys-cdo/poc-pdf-assessor | Triage: effort estimate, reach, retire/replace/remediate, shell-tag detection | none | Ideas only |
| ethan87w/pdf-remediation | Hash-keyed AI cache; honest-scope framing | none | Ideas only |
| Rafael-Calderon/pdf-remediation-pipeline | Not recommended as a model (metadata only, empty tag tree) | MIT | Matterhorn reference doc only |
| pdf-accessibility-remediation/pdf-accessibility-remediation | Closest match to this spec: OCR (skip mode) → ODL tag → model returns a JSON work order → deterministic apply → veraPDF before/after with fixed / still-failing / newly-failing classification; cost estimate and confirmation; resume by run label; stops PDF 2.0 files for a human | none found | Ideas only |
| MatiasAwdew/pdf-accessibility-remediator | Local review cards app; few-shot learning from reviewer corrections; font embedding; veraPDF validate-and-fix loop; heading/table/link repairs | none found | Ideas only |
| Mohakgarg5/KCP_PDF_REMEDIATION | Content-stream tagger for InDesign-style documents (about 3.1k lines) plus 2.3k-line post-processor; many dated regression tests | none found | Ideas only |
| anitasrinivasan/ada-pdf-compliance | Claude Code plugin; slide-deck focus; bookmarks from page titles; link descriptions | MIT | May adapt with attribution |
| opendataloader-project/opendataloader-pdf | Default engine candidate; ships agent skills and a bundled veraPDF core | Apache-2.0 (veraPDF parts MPL-2.0) | May use as a dependency |

---

## Changelog

- **v0.2 (2026-10-16):** Added Gate 2 check 13 (placeholder alt text). Recorded that the free ODL tier outputs Tagged PDF only and that PDF/UA export is paid. Promoted font repair into scope. Added optional local picture-description path, cost-confirmation before paid calls, new reference projects and risks, and the Phase 0 note that ODL/PDFix/accesspdf were already measured once (RESEARCH.md §7–§9, `eval/poc/`).
- **v0.1 (2026-10-15):** Initial spec from the seven-tool comparison.
