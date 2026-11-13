#!/usr/bin/env python3
"""Python reference adapter; install TexTeller separately and provision local files."""
import os
os.environ['HF_HUB_OFFLINE'] = '1'
os.environ['TRANSFORMERS_OFFLINE'] = '1'
import contextlib
import importlib.metadata
import json
from pathlib import Path
import sys
import time


def main():
    model_dir, tokenizer_dir, max_tokens = sys.argv[1:]
    for path in [model_dir,tokenizer_dir]:
        if not Path(path).is_dir():
            raise ValueError('local directory required: '+path)
    request = json.load(sys.stdin)
    if request['schema_version'] != 'omniattach/formula-worker/0.1':
        raise ValueError('unsupported protocol')
    # Keep package import/load noise out of the JSON transport.
    with contextlib.redirect_stdout(sys.stderr):
        import torch
        from texteller import load_model, load_tokenizer, img2latex
        start = time.perf_counter()
        model = load_model(model_dir=model_dir,use_onnx=False)
        tokenizer = load_tokenizer(tokenizer_dir=tokenizer_dir)
        model = model.to(torch.device('cpu'))
        model.eval()
        load_ms = (time.perf_counter()-start)*1000
        results = []
        for image in request['images']:
            start = time.perf_counter()
            try:
                with torch.inference_mode():
                    latex = img2latex(model,tokenizer,[image['path']],device=torch.device('cpu'),
                        out_format='latex',keep_style=True,max_tokens=int(max_tokens),num_beams=1)[0]
                result = dict(id=image['id'],status='ok',latex=latex)
            except Exception as error:
                result = dict(id=image['id'],status='error',error=str(error))
            result['elapsed_ms'] = (time.perf_counter()-start)*1000
            results.append(result)
    print(json.dumps(dict(results=results,metadata=dict(backend='texteller-python',
        package_version=importlib.metadata.version('texteller'),torch_version=torch.__version__,
        device='cpu',load_ms=load_ms,max_tokens=int(max_tokens),completion='unknown',
        postprocessing='upstream img2latex formatting retained; keep_style=True; not raw token parity'))))


if __name__ == '__main__':
    main()
