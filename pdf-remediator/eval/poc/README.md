# Proof-of-concept scripts (session of 2026-10-16)

Small helper scripts used to produce the numbers in `../../RESEARCH.md` §8. Not production code.

| File | Purpose |
|---|---|
| `VerapdfCheck.java` | Runs veraPDF's PDF/UA-1 validator and prints failed rules and counts |
| `patch_ua.py` | Catalog-level PDF/UA fixes with pikepdf (stand-in for SPEC S5) |
| `inspect_pdf.py` | Prints tag counts, Lang, Title, DisplayDocTitle, XMP `pdfuaid:part`, figures with alt |

## Preferred on desktop: install the real veraPDF CLI

`verapdf --flavour ua1 file.pdf` (confirm flags with `verapdf --help`). Use `VerapdfCheck.java` only if the CLI is unavailable.

## Fallback: veraPDF core from the opendataloader-pdf jar

```bash
pip install opendataloader-pdf pikepdf
JAR=$(python -c "import opendataloader_pdf,os;print(os.path.join(os.path.dirname(opendataloader_pdf.__file__),'jar','opendataloader-pdf-cli.jar'))")
mkdir -p libs classes
# JDK 11+ has no JAXB. Download from Maven Central into libs/:
#   javax/xml/bind/jaxb-api/2.3.1/jaxb-api-2.3.1.jar
#   com/sun/xml/bind/jaxb-core/2.3.0.1/jaxb-core-2.3.0.1.jar
#   com/sun/xml/bind/jaxb-impl/2.3.0.1/jaxb-impl-2.3.0.1.jar
javac -cp "$JAR" -d classes VerapdfCheck.java
java -cp "$JAR:classes:libs/*" VerapdfCheck file1.pdf file2.pdf
```

On Windows replace `:` with `;` in the classpath.

## Typical run

```bash
opendataloader-pdf --format tagged-pdf -o out input.pdf          # free tier: Tagged PDF only
python patch_ua.py out/input_tagged.pdf out/input_patched.pdf "Document title"
python inspect_pdf.py out/input_patched.pdf
java -cp "$JAR:classes:libs/*" VerapdfCheck out/input_patched.pdf
```

Last measured result: on the test flyer, ODL + `patch_ua.py` left only font rules (7.21.4.1-1, 7.21.4.2-2). See RESEARCH.md §8.
