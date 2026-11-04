#!/usr/bin/env python3
"""Export NB-Whisper to sherpa-onnx int8 by running sherpa-onnx's own export-onnx.py.

usage: export.py <sherpa-onnx scripts/whisper dir> <model.bin> <large-v3|medium> <outdir>

sherpa's script only loads OpenAI-named models, so we swap its load_model for the
NB-Whisper checkpoint (OpenAI format) and pass the matching base size so n_mels is right.
"""
import importlib.util, os, shutil, sys

src, ckpt, base, out = sys.argv[1:5]
assert base in ("large-v3", "medium"), base  # NB-Whisper large is large-v3 (128 mels, feature_size in its preprocessor_config.json)
os.makedirs(out, exist_ok=True)
os.chdir(out)
sys.path.insert(0, src)
spec = importlib.util.spec_from_file_location("sherpa_export", os.path.join(src, "export-onnx.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.load_model = lambda name: m.whisper.load_model(ckpt)
sys.argv = ["export-onnx.py", "--model", base]
from whisper.model import disable_sdpa  # same workaround as the script's __main__

with disable_sdpa():
    m.main()
# the layout whisper_nb.rs looks for
for kind in ("encoder", "decoder"):
    shutil.move(f"{base}-{kind}.int8.onnx", f"{kind}.int8.onnx")
shutil.move(f"{base}-tokens.txt", "tokens.txt")
assert all(os.path.isfile(f) for f in ("encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"))
