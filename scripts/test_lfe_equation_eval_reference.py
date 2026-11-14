"""Reference config tests use dummy artifacts, not models or downloads."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from lfe_equation_reference import FILES, REPO, REVISION, REQUIRED, artifact, build_config

class ReferenceTests(unittest.TestCase):
    def setUp(self):
        temporary=tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root=Path(temporary.name)
        for name in FILES:
            (self.root/name).write_text(name)
        self.worker=self.root/'worker.py'
        self.worker.write_text('# protocol stub, not inference\n')
        self.encoder=self.root/'encoder.onnx'; self.encoder.write_bytes(b'encoder')
        self.decoder=self.root/'decoder.onnx'; self.decoder.write_bytes(b'decoder')
        self.base=dict(backends=[dict(id='bobine-texteller-cpu',
            argv=[sys.executable,str(self.encoder),str(self.decoder),str(self.root/'tokenizer.json'),'512'],
            artifacts=[artifact(p) for p in [self.encoder,self.decoder,self.root/'tokenizer.json']],
            provenance=dict(model_repository=REPO,model_revision=REVISION))])

    def build(self, base=None, packages=None):
        return build_config(self.base if base is None else base,self.root,self.worker,sys.executable,
                            REQUIRED if packages is None else packages)

    def test_preserves_rust_and_hashes_reference(self):
        original=copy.deepcopy(self.base)
        result=self.build()
        self.assertEqual(self.base,original)
        self.assertEqual(result['backends'][0],original['backends'][0])
        reference=result['backends'][1]
        self.assertEqual(reference['argv'][-1],'512')
        self.assertEqual(len(reference['artifacts']),len(FILES)+2)
        self.assertEqual(reference['provenance']['model_revision'],REVISION)

    def test_venv_interpreter_symlink_is_preserved_in_argv(self):
        directory=self.root/'venv/bin';directory.mkdir(parents=True)
        link=directory/'python';link.symlink_to(sys.executable)
        result=build_config(self.base,self.root,self.worker,link,REQUIRED)
        self.assertEqual(result['backends'][1]['argv'][0],str(link.absolute()))

    def test_missing_weights_fail_without_download(self):
        (self.root/'model.safetensors').unlink()
        with self.assertRaises(FileNotFoundError): self.build()

    def test_changed_rust_artifact_fails(self):
        self.encoder.write_bytes(b'changed')
        with self.assertRaises(ValueError): self.build()

    def test_different_tokenizer_fails(self):
        other=self.root/'other-tokenizer.json';other.write_text('different')
        self.base['backends'][0]['argv'][3]=str(other)
        self.base['backends'][0]['artifacts'][2]=artifact(other)
        with self.assertRaises(ValueError): self.build()

    def test_wrong_versions_or_provenance_fail(self):
        with self.assertRaises(ValueError): self.build(packages=dict(REQUIRED,texteller='0.1.2'))
        self.base['backends'][0]['provenance']['model_revision']='main'
        with self.assertRaises(ValueError): self.build()

    def test_ambiguous_base_or_undeclared_model_fails(self):
        with self.assertRaises(ValueError): self.build(base=dict(backends=[]))
        with self.assertRaises(ValueError): self.build(base=dict(backends=self.base['backends']*2))
        self.base['backends'][0]['artifacts'].pop()
        with self.assertRaises(ValueError): self.build()

    def test_cli_refuses_existing_output_before_importing_models(self):
        output=self.root/'config.json';output.write_text('unchanged')
        run=subprocess.run([sys.executable,str(Path(__file__).with_name('lfe_equation_reference.py')),
                            '--output',str(output)],capture_output=True,text=True)
        self.assertNotEqual(run.returncode,0)
        self.assertEqual(output.read_text(),'unchanged')
        self.assertIn('output already exists',run.stderr)

if __name__ == '__main__': unittest.main()
