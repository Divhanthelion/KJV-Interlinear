"""Speaker embeddings (WeSpeaker ResNet34-LM, ONNX, CC BY 4.0) to tell voices
apart: who is speaking a line, and how close two cast voices are. The model
is fetched from Hugging Face (Wespeaker/wespeaker-voxceleb-resnet34-LM)."""
import glob
import os

import numpy as np

_sess = None


def _model_path():
    hits = glob.glob(os.path.expanduser(
        r"~\.cache\huggingface\hub\models--Wespeaker--wespeaker-voxceleb-resnet34-LM\snapshots\*\voxceleb_resnet34_LM.onnx"))
    if hits:
        return hits[0]
    from huggingface_hub import hf_hub_download
    return hf_hub_download("Wespeaker/wespeaker-voxceleb-resnet34-LM", "voxceleb_resnet34_LM.onnx")


def embed(samples: np.ndarray) -> np.ndarray:
    """A unit vector for 16 kHz mono float samples."""
    global _sess
    import kaldi_native_fbank as knf
    import onnxruntime as ort
    if _sess is None:
        _sess = ort.InferenceSession(_model_path(), providers=["CPUExecutionProvider"])
    opts = knf.FbankOptions()
    opts.frame_opts.dither = 0.0
    opts.frame_opts.samp_freq = 16000
    opts.mel_opts.num_bins = 80
    fb = knf.OnlineFbank(opts)
    fb.accept_waveform(16000, (samples * 32768.0).tolist())
    fb.input_finished()
    feats = np.stack([fb.get_frame(i) for i in range(fb.num_frames_ready)]).astype(np.float32)
    feats -= feats.mean(axis=0, keepdims=True)
    e = _sess.run(None, {_sess.get_inputs()[0].name: feats[None]})[0][0]
    return e / np.linalg.norm(e)
