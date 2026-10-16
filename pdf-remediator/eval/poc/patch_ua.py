# Proof of concept for SPEC S5: catalog-level PDF/UA fixes on an engine's tagged output.
import sys, pikepdf
src, dst, title = sys.argv[1:4]
with pikepdf.open(src) as pdf:
    r = pdf.Root
    if "/Lang" not in r: r.Lang = pikepdf.String("en-US")
    r.MarkInfo = pikepdf.Dictionary(Marked=True)
    if "/ViewerPreferences" not in r: r.ViewerPreferences = pikepdf.Dictionary()
    r.ViewerPreferences.DisplayDocTitle = True
    with pdf.open_metadata(set_pikepdf_as_editor=False) as m:
        m["dc:title"] = title
        m["pdfuaid:part"] = "1"
    pdf.docinfo["/Title"] = pikepdf.String(title)
    for p in pdf.pages: p.Tabs = pikepdf.Name("/S")
    pdf.save(dst)
