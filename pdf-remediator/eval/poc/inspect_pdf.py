import sys, pikepdf
from collections import Counter
def walk(node, c, figs, depth=0, maxd=60):
    if depth>maxd: return
    if isinstance(node, pikepdf.Dictionary):
        s = node.get("/S")
        if s is not None:
            c[str(s)] += 1
            if str(s)=="/Figure":
                figs.append(("/Alt" in node and str(node["/Alt"]) or None))
        k = node.get("/K")
        if k is not None:
            if isinstance(k, pikepdf.Array):
                for x in k: walk(x,c,figs,depth+1)
            else: walk(k,c,figs,depth+1)
    elif isinstance(node, pikepdf.Array):
        for x in node: walk(x,c,figs,depth+1)
for path in sys.argv[1:]:
    pdf = pikepdf.open(path)
    r = pdf.Root
    print("==", path, "pages", len(pdf.pages))
    mi = r.get("/MarkInfo"); print(" MarkInfo", dict(mi) if mi is not None else None)
    print(" Lang", r.get("/Lang"), "| Title", pdf.docinfo.get("/Title"))
    vp = r.get("/ViewerPreferences"); print(" DisplayDocTitle", vp.get("/DisplayDocTitle") if vp is not None else None)
    print(" Outlines", "/Outlines" in r)
    st = r.get("/StructTreeRoot")
    if st is None: print(" StructTree: NONE"); continue
    c=Counter(); figs=[]
    walk(st, c, figs)
    print(" Tags", dict(c))
    print(" Figures:", len(figs), "with alt:", sum(1 for f in figs if f))
    with pdf.open_metadata() as m:
        print(" XMP pdfuaid:part", m.get("pdfuaid:part"))
