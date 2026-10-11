#!/usr/bin/env python3
"""Build no code; package one existing Linux x86_64 hud-rs executable locally."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True, timeout=60).stdout


def package(repo, binary, output, cargo, fake_agent=None):
    repo=repo.resolve(); binary=binary.resolve(); output=output.absolute()
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'amd64'):
        raise ValueError('only current Linux x86_64 host is supported')
    if output.exists() or output.is_symlink():
        raise ValueError('output must be a new explicit path')
    if not output.parent.is_dir():
        raise ValueError('output parent must already exist')
    header=binary.read_bytes()[:64]
    if header[:5] != b'\x7fELF\x02' or header[5] != 1 or int.from_bytes(header[18:20], 'little') != 62:
        raise ValueError('binary must be Linux ELF64 little-endian x86_64')
    manifest=repo/'Cargo.toml'
    metadata=json.loads(run([cargo,'metadata','--manifest-path',str(manifest),'--locked','--offline','--format-version','1'],repo))
    tree=run([cargo,'tree','--manifest-path',str(manifest),'--locked','--offline','--package','hud-rs','--edges','normal,build','--target','x86_64-unknown-linux-gnu','--prefix','none','--format','{p}'],repo)
    active={(m.group(1),m.group(2)) for line in tree.splitlines() if (m:=re.match(r'([^ ]+) v([^ ]+)',line))}
    with tempfile.TemporaryDirectory(prefix='hud-package-') as tmp:
        stage=Path(tmp)/'hud-rs-linux-x86_64'; stage.mkdir()
        shutil.copy2(binary,stage/'hud-rs'); (stage/'hud-rs').chmod(0o755)
        if fake_agent is not None:
            fake_agent = fake_agent.resolve()
            info = json.loads(run([str(binary), '--build-info'], repo))
            digest = hashlib.sha256(fake_agent.read_bytes()).hexdigest()
            if info.get('fake_agent_sha256') != digest:
                raise ValueError('fake digest does not match compile-time pin')
            shutil.copy2(fake_agent, stage/'fake_agent'); (stage/'fake_agent').chmod(0o755)
        for name in ('LICENSE','README.md','README.pt-BR.md'):
            shutil.copy2(repo/name,stage/name)
        shutil.copy2(repo/'Cargo.lock',stage/'Cargo.lock')
        shutil.copytree(repo/'docs',stage/'docs')
        notices=stage/'licenses'; notices.mkdir(); inventory=[]
        for p in sorted(metadata['packages'],key=lambda p:(p['name'],p['version'])):
            if not p.get('source') or (p['name'],p['version']) not in active: continue
            if not p['source'].startswith('registry+'):
                raise ValueError('non-registry dependency refused')
            root=Path(p['manifest_path']).parent
            files=[f for f in root.rglob('*') if f.is_file() and not f.is_symlink() and len(f.relative_to(root).parts)<=3 and f.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE'))]
            if p.get('license_file'):
                declared=(root/p['license_file']).resolve()
                if declared.is_relative_to(root.resolve()) and declared.is_file() and declared not in files: files.append(declared)
            if not files: raise ValueError(f"missing full license files: {p['name']} {p['version']}")
            destination=notices/f"{p['name']}-{p['version']}"; destination.mkdir()
            for f in files:
                relative=f.relative_to(root); target=destination/relative; target.parent.mkdir(parents=True,exist_ok=True); shutil.copyfile(f,target)
            inventory.append({'name':p['name'],'version':p['version'],'license':p.get('license'),'source':p['source'],'files':[str(f.relative_to(root)) for f in files]})
        (stage/'dependency-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
        for source in ('PACKAGING.en.md','PACKAGING.pt-BR.md'):
            document=Path(__file__).with_name(source)
            shutil.copyfile(document,stage/source)
        # Bundled guides have a different layout from the checkout. Keep source
        # references readable without creating links to files absent from the archive.
        project_documents = [stage / name for name in ("README.md", "README.pt-BR.md", "PACKAGING.en.md", "PACKAGING.pt-BR.md")]
        project_documents.extend((stage / "docs").rglob("*.md"))
        for document in project_documents:
            content = document.read_text()
            if document.parent == stage and document.name.startswith("PACKAGING."):
                content = content.replace("](../docs/", "](docs/")
            def bundled_link(match):
                label, target = match.groups()
                if "://" in target or target.startswith("#"):
                    return match.group(0)
                relative = target.split("#", 1)[0]
                destination = (document.parent / relative).resolve()
                if destination.is_relative_to(stage.resolve()) and destination.exists():
                    return match.group(0)
                return f"{label} (checkout: `{target}`)"
            content = re.sub(r"\[([^\]]+)\]\(([^\s)]+)\)", bundled_link, content)
            document.write_text(content)
        checksums=''.join(f"{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.relative_to(stage)}\n" for f in sorted(stage.rglob('*')) if f.is_file())
        (stage/'SHA256SUMS').write_text(checksums)
        # Smoke outside checkout with isolated HOME and no Python in the package.
        home=Path(tmp)/'home'; home.mkdir()
        environment={'HOME':str(home),'PATH':'/usr/bin:/bin','LANG':'C.UTF-8'}
        executable=str(stage/'hud-rs')
        for args in [['--help'],['--version']]:
            result=subprocess.run([executable,*args],cwd=tmp,env=environment,capture_output=True,text=True,timeout=10)
            if result.returncode: raise ValueError(f'smoke failed: {args}: {result.stderr}')
        for args in [['--unknown','value'], ['--state-dir'], ['--state-dir',str(Path(tmp)/'../escape')]]:
            result=subprocess.run([executable,*args],cwd=tmp,env=environment,capture_output=True,text=True,timeout=10)
            if not result.returncode: raise ValueError(f'invalid option accepted: {args}')
        state=Path(tmp)/'state'; state.mkdir(mode=0o700); (state/'layout.state').write_text('corrupt'); (state/'layout.state').chmod(0o600)
        result=subprocess.run([executable,'--state-dir',str(state)],cwd=tmp,env=environment,capture_output=True,text=True,timeout=10)
        if not result.returncode or (state/'layout.state').read_text() != 'corrupt': raise ValueError('corrupt state smoke failed')
        assert not list(stage.rglob('*.py')) and not list(stage.rglob('*runner-fixture*'))
        # Exclusive output creation protects existing deliverables.
        with output.open('xb') as stream:
            with tarfile.open(fileobj=stream,mode='w:gz') as archive: archive.add(stage,arcname=stage.name)
    return {'archive':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'license_packages':len(inventory),'smoke':'help/version/invalid-options/corrupt-state passed','target':'Linux x86_64 only'}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo',required=True,type=Path,help='Rust workspace root')
    p.add_argument('--binary',required=True,type=Path)
    p.add_argument('--output',required=True,type=Path)
    p.add_argument('--cargo',required=True)
    p.add_argument('--fake-agent',type=Path)
    a=p.parse_args()
    print(json.dumps(package(a.repo,a.binary,a.output,a.cargo,a.fake_agent),indent=2))
if __name__=='__main__': main()
