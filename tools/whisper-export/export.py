#!/usr/bin/env python3
"""Export NB-Whisper to sherpa-onnx int8 by running sherpa-onnx's own export-onnx.py.

usage: export.py <sherpa-onnx scripts/whisper dir> <safetensors dir> <large-v3|medium> <outdir>

NB-Whisper's `model.bin` is a CTranslate2 file, not a PyTorch checkpoint, so the weights come
from the repo's Hugging Face safetensors shards instead. safetensors cannot run code when loaded
(unlike `torch.load(weights_only=False)`, which this script never uses). They are renamed to
OpenAI's layout, the inverse of transformers' `convert_openai_to_hf.py`, loaded into
`whisper.model.Whisper`, and handed to sherpa's script, which only loads OpenAI-named models.
"""
import importlib.util
import json
import os
import shutil
import sys

from safetensors.torch import load_file

src, hf_dir, base, out = sys.argv[1:5]
assert base in ("large-v3", "medium", "small"), base  # large is large-v3 (128 mels); medium/small 80 (feature_size in preprocessor_config.json)
hf_dir = os.path.abspath(hf_dir)

# HF name -> OpenAI name; applied in this order (longer, more specific first).
RENAMES = [
    (".self_attn.q_proj", ".attn.query"),
    (".self_attn.k_proj", ".attn.key"),
    (".self_attn.v_proj", ".attn.value"),
    (".self_attn.out_proj", ".attn.out"),
    (".self_attn_layer_norm", ".attn_ln"),
    (".encoder_attn.q_proj", ".cross_attn.query"),
    (".encoder_attn.k_proj", ".cross_attn.key"),
    (".encoder_attn.v_proj", ".cross_attn.value"),
    (".encoder_attn.out_proj", ".cross_attn.out"),
    (".encoder_attn_layer_norm", ".cross_attn_ln"),
    (".final_layer_norm", ".mlp_ln"),
    (".fc1", ".mlp.0"),
    (".fc2", ".mlp.2"),
    (".layers.", ".blocks."),
    ("encoder.layer_norm.", "encoder.ln_post."),
    ("decoder.layer_norm.", "decoder.ln."),
    ("decoder.embed_tokens", "decoder.token_embedding"),
    ("encoder.embed_positions.weight", "encoder.positional_embedding"),
    ("decoder.embed_positions.weight", "decoder.positional_embedding"),
]


def openai_state(hf_dir):
    index_path = os.path.join(hf_dir, "model.safetensors.index.json")
    if os.path.isfile(index_path):  # sharded (large)
        shards = sorted(set(json.load(open(index_path))["weight_map"].values()))
    else:  # one file (medium, small)
        shards = ["model.safetensors"]
    hf = {}
    for shard in shards:
        hf.update(load_file(os.path.join(hf_dir, shard)))
    state = {}
    for key, tensor in hf.items():
        if key == "proj_out.weight":  # tied to the token embedding in OpenAI's model
            continue
        name = key.removeprefix("model.")
        for old, new in RENAMES:
            name = name.replace(old, new)
        state[name] = tensor
    return state


def dims(hf_dir):
    c = json.load(open(os.path.join(hf_dir, "config.json")))
    return dict(
        n_mels=c["num_mel_bins"], n_vocab=c["vocab_size"],
        n_audio_ctx=c["max_source_positions"], n_audio_state=c["d_model"],
        n_audio_head=c["encoder_attention_heads"], n_audio_layer=c["encoder_layers"],
        n_text_ctx=c["max_target_positions"], n_text_state=c["d_model"],
        n_text_head=c["decoder_attention_heads"], n_text_layer=c["decoder_layers"],
    )


def load_nb_whisper(_name):
    from whisper.model import ModelDimensions, Whisper

    model = Whisper(ModelDimensions(**dims(hf_dir)))
    missing, unexpected = model.load_state_dict(openai_state(hf_dir), strict=False)
    # alignment_heads is a buffer OpenAI sets separately; nothing else may be missing.
    assert set(missing) <= {"alignment_heads"} and not unexpected, (missing, unexpected)
    return model


os.makedirs(out, exist_ok=True)
os.chdir(out)
sys.path.insert(0, src)
spec = importlib.util.spec_from_file_location("sherpa_export", os.path.join(src, "export-onnx.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.load_model = load_nb_whisper
sys.argv = ["export-onnx.py", "--model", base]
# The dynamo exporter (default in newer torch) ignores dynamic_axes and bakes static shapes
# into the decoder (Reshape {1,4,1280} vs {1280} at runtime); the legacy exporter honours them.
import torch

_export = torch.onnx.export
torch.onnx.export = lambda *a, **k: _export(*a, **{**k, "dynamo": False})
from whisper.model import disable_sdpa  # same workaround as the script's __main__

with disable_sdpa():
    m.main()
# the layout whisper_nb.rs looks for
for kind in ("encoder", "decoder"):
    shutil.move(f"{base}-{kind}.int8.onnx", f"{kind}.int8.onnx")
shutil.move(f"{base}-tokens.txt", "tokens.txt")
assert all(os.path.isfile(f) for f in ("encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"))
