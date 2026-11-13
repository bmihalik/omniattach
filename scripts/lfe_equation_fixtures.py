#!/usr/bin/env python3
"""Generate local synthetic equation fixtures; never run an OCR model."""
import argparse
import hashlib
import importlib.metadata
import io
import json
import platform
import shutil
import subprocess
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
matplotlib.rcdefaults()
matplotlib.rcParams.update({"pdf.fonttype": 42, "mathtext.fontset": "dejavusans", "figure.autolayout": False})
from matplotlib.figure import Figure
from matplotlib.backends.backend_agg import FigureCanvasAgg
from matplotlib.textpath import TextPath
from matplotlib.patches import PathPatch
from matplotlib.transforms import Affine2D
from PIL import Image
from pypdf import PdfReader, PdfWriter
from pypdf.generic import ArrayObject, FloatObject
from reportlab import rl_config
rl_config.useA85 = 0
from reportlab.pdfgen.canvas import Canvas
from reportlab.lib.utils import ImageReader

SIZE = (612, 792)
SLOT = [60, 590, 552, 710]
FORMULAS = [
    ("equality", r"x+y=z"),
    ("fraction", r"\frac{a+b}{c-d}"),
    ("nested", r"\frac{1}{1+\frac{1}{x}}"),
    ("root", r"\sqrt{x^2+y^2}"),
    ("integral", r"\int_0^1 x^2\,dx=\frac{1}{3}"),
    ("sum", r"\sum_{i=1}^{n}i=\frac{n(n+1)}{2}"),
    ("limit", r"\lim_{x\to0}\frac{\sin x}{x}=1"),
    ("scripts", r"\alpha_i^2+\beta_j^2=\gamma_{ij}"),
]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canvas(path):
    return Canvas(str(path), pagesize=SIZE, invariant=1, pageCompression=1)


def render_formula(latex):
    fig = Figure(figsize=(4.8, 1), dpi=150)
    FigureCanvasAgg(fig)
    fig.text(.03, .4, '$' + latex + '$', fontsize=22)
    buf = io.BytesIO()
    fig.savefig(buf, format="png", facecolor="white", metadata={"Software": "omniattach-lfe4a"})
    buf.seek(0)
    return Image.open(buf).convert("RGB")


def native_page(latex, path, outlines=False):
    fig = Figure(figsize=(SIZE[0]/72, SIZE[1]/72), dpi=72)
    FigureCanvasAgg(fig)
    if outlines:
        ax = fig.add_axes([0, 0, 1, 1], xlim=(0, SIZE[0]), ylim=(0, SIZE[1]))
        ax.set_axis_off()
        p = TextPath((0, 0), '$' + latex + '$', size=22)
        box = p.get_extents()
        scale = min(1, 460/box.width, 80/box.height)
        trans = Affine2D().translate(-box.x0, -box.y0).scale(scale).translate(72, 620)
        ax.add_patch(PathPatch(p, transform=trans + ax.transData, color="black", linewidth=0))
    else:
        fig.text(72/SIZE[0], 650/SIZE[1], '$' + latex + '$', fontsize=22)
    fig.savefig(path, format="pdf", metadata={"CreationDate": None, "ModDate": None})


def combine(paths, out):
    writer = PdfWriter()
    for path in paths:
        writer.add_page(PdfReader(path).pages[0])
    writer.write(out)


ADVANCED = [
    ("matrix", r"A=\begin{pmatrix}a&b\\c&d\end{pmatrix}"),
    ("cases", r"f(x)=\begin{cases}x^2&x\ge0\\-x&x<0\end{cases}"),
    ("aligned", r"\begin{aligned}(a+b)^2&=a^2+2ab+b^2\\&=(a+b)(a+b)\end{aligned}"),
    ("derivative", r"\frac{\partial^2 u}{\partial t^2}=c^2\nabla^2u"),
]


def generate(root, latex=False):
    include_latex = latex
    if include_latex and shutil.which("pdflatex") is None:
        raise RuntimeError("--latex requires pdflatex; no automatic installation is attempted")
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    (root / "formula-images").mkdir()
    docs, occurrences = [], []
    def record(pdf, page, ident, latex, representation, bbox=SLOT, image=None, source_id=None):
        occurrences.append(dict(id=f"{pdf}:{page}:{ident}", pdf=pdf, page=page,
            formula_id=source_id or ident, latex=latex, role="equation", display=True,
            representation=representation, bbox_pdf=bbox, bbox_kind="allocated_region_not_ink",
            coordinate_space="unrotated_pdf_default_user_space", formula_image=image))
    import tempfile
    with tempfile.TemporaryDirectory() as scratch:
        scratch = Path(scratch)
        for outlines in [False, True]:
            name = "outlines.pdf" if outlines else "native.pdf"
            pages = []
            for number, (ident, latex) in enumerate(FORMULAS, 1):
                page = scratch / f"page-{number}.pdf"
                native_page(latex, page, outlines)
                pages.append(page)
                record(name, number, ident, latex, "vector_paths" if outlines else "native_mathtext")
            combine(pages, root / name)
            docs.append(name)
    for ident, latex in FORMULAS:
        render_formula(latex).save(root / "formula-images" / f"{ident}.png")
    name = "raster.pdf"
    c = canvas(root / name)
    for number, (ident, latex) in enumerate(FORMULAS, 1):
        image = f"formula-images/{ident}.png"
        c.drawString(72, 740, "Synthetic prose before the formula.")
        c.drawImage(ImageReader(str(root / image)), 60, 600, width=480, height=100)
        c.drawString(72, 550, "Synthetic prose after the formula.")
        c.showPage()
        record(name, number, ident, latex, "image_xobject", [60, 600, 540, 700], image)
    c.save()
    docs.append(name)
    name = "repeated.pdf"
    c = canvas(root / name)
    image = "formula-images/fraction.png"
    for index, y in enumerate([600, 400], 1):
        c.drawImage(ImageReader(str(root / image)), 60, y, width=480, height=100)
        record(name, 1, f"fraction-occurrence-{index}", FORMULAS[1][1], "reused_image_xobject", [60,y,540,y+100], image, "fraction")
    c.save()
    docs.append(name)
    name = "transformed.pdf"
    c = canvas(root / name)
    c.saveState()
    c.translate(120, 300)
    c.rotate(90)
    c.drawImage(ImageReader(str(root / image)), 0, 0, width=240, height=50)
    c.restoreState()
    c.save()
    record(name, 1, "fraction-rotated-placement", FORMULAS[1][1], "rotated_scaled_image", [70,300,120,540], image, "fraction")
    docs.append(name)
    name = "page-transform.pdf"
    writer = PdfWriter()
    page = PdfReader(root / "raster.pdf").pages[0]
    page.cropbox = ArrayObject([FloatObject(x) for x in [24, 36, 588, 756]])
    page.rotate(90)
    writer.add_page(page)
    writer.write(root / name)
    record(name, 1, "equality-page-transform", FORMULAS[0][1], "cropbox_rotate", [60,600,540,700], "formula-images/equality.png", "equality")
    docs.append(name)
    # Independent rendering implementation is also used by the validator.
    import pypdfium2 as pdfium
    name = "scanned.pdf"
    c = canvas(root / name)
    source = pdfium.PdfDocument(str(root / "raster.pdf"))
    for number, (ident, latex) in enumerate(FORMULAS, 1):
        bitmap = source[number-1].render(scale=1.5).to_pil().convert("RGB")
        c.drawImage(ImageReader(bitmap), 0, 0, width=SIZE[0], height=SIZE[1])
        c.showPage()
        record(name, number, ident, latex, "full_page_raster", [60,600,540,700])
    source.close()
    c.save()
    docs.append(name)
    name = "negatives.pdf"
    c = canvas(root / name)
    for y, text in [(700,"Ordinary prose without mathematics."), (650,"Code-like assignment: count = total"), (600,"Table headings: Name  Value  Notes")]:
        c.drawString(72, y, text)
    c.save()
    docs.append(name)
    if include_latex:
        compiler = shutil.which("pdflatex")
        if compiler is None:
            raise RuntimeError("--latex requires pdflatex; no automatic installation is attempted")
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            pages = []
            for number, (ident, expression) in enumerate(ADVANCED, 1):
                tex = (r"\documentclass{article}"
                       r"\usepackage[paperwidth=612bp,paperheight=792bp,margin=72bp]{geometry}"
                       r"\usepackage{amsmath,amssymb}\pagestyle{empty}"
                       r"\begin{document}\noindent\[" + expression + r"\]\end{document}")
                (tmp / "formula.tex").write_text(tex)
                result = subprocess.run([compiler, "-no-shell-escape", "-interaction=nonstopmode",
                    "-halt-on-error", "formula.tex"], cwd=tmp, capture_output=True, timeout=60)
                if result.returncode:
                    raise RuntimeError(result.stdout.decode(errors="replace")[-4000:])
                target = tmp / f"advanced-{number}.pdf"
                shutil.copyfile(tmp / "formula.pdf", target)
                if len(PdfReader(target).pages) != 1:
                    raise RuntimeError("advanced fixture unexpectedly spans multiple pages")
                pages.append(target)
                record("latex.pdf", number, ident, expression, "native_latex", [60,580,552,735])
            combine(pages, root / "latex.pdf")
        docs.append("latex.pdf")
    versions = {x: importlib.metadata.version(x) for x in ["matplotlib", "reportlab", "pypdf", "Pillow", "pypdfium2"]}
    manifest = dict(schema_version="omniattach/lfe-equation-fixtures/0.1", generator="lfe4a/1",
        environment=dict(python=platform.python_version(), packages=versions),
        fontset="dejavusans", latex_enabled=include_latex,
        latex_toolchain=(subprocess.run([compiler,"--version"],capture_output=True,text=True).stdout.splitlines()[0] if include_latex else None), source_formulas=[dict(id=i,latex=t) for i,t in FORMULAS + (ADVANCED if include_latex else [])],
        documents=[dict(path=n, sha256=sha(root/n), pages=len(PdfReader(root/n).pages),
            negative=(n=="negatives.pdf")) for n in docs], occurrences=occurrences,
        assets=[dict(path=p.relative_to(root).as_posix(),sha256=sha(p)) for p in sorted((root/"formula-images").glob("*.png"))])
    (root / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new directory; never overwrites an existing corpus")
    parser.add_argument("--latex", action="store_true", help="also generate advanced amsmath fixtures using pdflatex")
    args = parser.parse_args()
    manifest = generate(args.output, latex=args.latex)
    print(json.dumps({"documents": len(manifest["documents"]), "occurrences": len(manifest["occurrences"])}))
