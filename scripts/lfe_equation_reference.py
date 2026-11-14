#!/usr/bin/env python3
"""Provision an opt-in Python reference config; never run inference here."""
import argparse
import copy
import hashlib
import importlib.metadata
import json
from pathlib import Path
import platform
import sys

REPO = 'opticsWolf/texteller3-onnx'
REVISION = '5f817336d600ae880205362e16092994aecd01e1'
FILES = ('model.safetensors', 'config.json', 'generation_config.json',
         'preprocessor_config.json', 'tokenizer.json', 'tokenizer_config.json',
         'special_tokens_map.json', 'added_tokens.json', 'vocab.json', 'merges.txt')
REQUIRED = {'texteller': '1.0.2', 'transformers': '4.47.0', 'optimum': '1.24.0'}


def artifact(path):
    path = Path(path).resolve(strict=True)
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            h.update(block)
    return dict(path=str(path), sha256=h.hexdigest())


def check_base(base):
    if len(base.get('backends', [])) != 1:
        raise ValueError('base config must contain exactly the working Rust backend')
    backend = base['backends'][0]
    if len(backend.get('argv', [])) != 5 or int(backend['argv'][-1]) <= 0:
        raise ValueError('expected Bobine argv: executable encoder decoder tokenizer max_tokens')
    if not backend.get('artifacts'):
        raise ValueError('Rust artifacts must be declared')
    for declared in backend['artifacts']:
        if artifact(declared['path'])['sha256'] != declared['sha256']:
            raise ValueError('Rust artifact hash mismatch: '+declared['path'])
    if not Path(backend['argv'][0]).is_file():
        raise ValueError('Rust executable not found')
    for model in backend['argv'][1:4]:
        resolved = str(Path(model).resolve(strict=True))
        if not any(str(Path(a['path']).resolve()) == resolved for a in backend['artifacts']):
            raise ValueError('Rust model/tokenizer is not a declared artifact: '+model)
    provenance = backend.get('provenance', {})
    if provenance.get('model_repository') != REPO or provenance.get('model_revision') != REVISION:
        raise ValueError('base model provenance must match the pinned reference repository/revision')
    return backend


def build_config(base, model_dir, worker, python, packages):
    rust = check_base(base)
    for name, version in REQUIRED.items():
        if packages.get(name) != version:
            raise ValueError(f'reference requires {name}=={version}')
    model_dir = Path(model_dir).resolve(strict=True)
    assets = [artifact(model_dir/name) for name in FILES]
    if artifact(model_dir/'tokenizer.json')['sha256'] != artifact(rust['argv'][3])['sha256']:
        raise ValueError('reference and Rust tokenizer.json bytes differ')
    python = Path(python).absolute()
    worker = Path(worker).resolve(strict=True)
    if not python.is_file() or not worker.is_file():
        raise ValueError('reference interpreter and worker must be files')
    result = copy.deepcopy(base)
    identifier = 'texteller-python-cpu'
    if rust['id'] == identifier:
        raise ValueError('reference backend ID collides with Rust backend')
    result['backends'].append(dict(
        id=identifier,
        argv=[str(python), str(worker), str(model_dir), str(model_dir), rust['argv'][-1]],
        artifacts=assets+[artifact(worker), artifact(python)],
        provenance=dict(model_repository=REPO, model_revision=REVISION,
                        python=platform.python_version(), packages=packages,
                        comparison='end-to-end CPU pipelines; upstream preprocessing/formatting retained; not raw token or numerical parity')))
    return result


def main():
    home = Path.home()/'.local/share/omniattach/lfe4c'
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, default=home/'config.json')
    parser.add_argument('--model-dir', type=Path, default=home/'texteller3-onnx')
    parser.add_argument('--output', type=Path, default=home/'comparison-config.json')
    parser.add_argument('--download', action='store_true', help='explicitly download missing reference assets from the pinned Hub revision')
    args = parser.parse_args()
    if args.output.exists():
        parser.error('output already exists; choose a new --output')
    base = json.loads(args.base.read_text())
    check_base(base)
    names = ('texteller', 'torch', 'torchvision', 'transformers', 'optimum', 'onnxruntime', 'huggingface-hub')
    packages = {name: importlib.metadata.version(name) for name in names}
    for name, version in REQUIRED.items():
        if packages[name] != version:
            parser.error(f'install {name}=={version} in the reference environment first')
    if args.download:
        from huggingface_hub import snapshot_download
        snapshot_download(repo_id=REPO, revision=REVISION,
                          local_dir=str(args.model_dir), allow_patterns=list(FILES))
    config = build_config(base, args.model_dir, Path(__file__).with_name('lfe_texteller_worker.py'), sys.executable, packages)
    with args.output.open('x') as stream:
        json.dump(config, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(json.dumps(dict(config=str(args.output.resolve()), backends=len(config['backends']), inference_run=False)))


if __name__ == '__main__':
    main()
