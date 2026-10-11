"""Local symlink cutover with checked rollback. Data restoration is opt-in, CAS guarded."""
import hashlib, json, os, shutil, stat, uuid
from pathlib import Path

def write_record(path, record):
    with Path(path).open('x') as output:
        os.chmod(path, 0o600)
        output.write(json.dumps(record, indent=2) + '\n')
        output.flush(); os.fsync(output.fileno())
    fd = os.open(Path(path).parent, os.O_RDONLY | os.O_DIRECTORY)
    try: os.fsync(fd)
    finally: os.close(fd)

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def atomic_link(path, target):
    path = Path(path)
    temporary = path.with_name('.' + path.name + '-' + uuid.uuid4().hex)
    try:
        temporary.symlink_to(target)
        os.replace(temporary, path)
        fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(fd)
        finally: os.close(fd)
    finally:
        if temporary.is_symlink(): temporary.unlink()

def capture(directory, links, data=()):
    directory = Path(directory)
    directory.mkdir(mode=0o700)
    record = {'links': [], 'data': []}
    for raw in links:
        path = Path(raw).absolute()
        if not path.is_symlink(): raise ValueError('expected an existing symlink')
        target = os.readlink(path)
        record['links'].append({'path': str(path), 'target': target, 'sha256': digest(path)})
    for number, raw in enumerate(data):
        path = Path(raw).absolute(); info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1: raise ValueError('regular single-link data required')
        destination = directory / ('data-' + str(number))
        with destination.open('xb') as output: output.write(path.read_bytes())
        destination.chmod(0o600)
        record['data'].append({'path': str(path), 'copy': destination.name, 'sha256': digest(destination), 'mode': stat.S_IMODE(info.st_mode)})
    write_record(directory / 'before.json', record)
    return record

def switch(directory, replacements):
    directory = Path(directory); record = json.loads((directory / 'before.json').read_text())
    by_path = {entry['path']: entry for entry in record['links']}
    for raw, target in replacements.items():
        path = str(Path(raw).absolute()); previous = by_path[path]
        if os.readlink(path) != previous['target'] or digest(path) != previous['sha256']: raise ValueError('launcher changed since backup')
        if not Path(target).is_file() or Path(target).is_symlink(): raise ValueError('candidate must be an installed regular file')
    # Persist rollback expectations before the first reversible mutation.
    current = {str(Path(p).absolute()): {'target': str(t), 'sha256': digest(t)} for p, t in replacements.items()}
    write_record(directory / 'after.json', current)
    for path, entry in current.items(): atomic_link(path, entry['target'])

def rollback(directory, restore_data=None):
    directory = Path(directory); record = json.loads((directory / 'before.json').read_text())
    current = json.loads((directory / 'after.json').read_text())
    for entry in record['links']:
        path = entry['path']
        if path not in current: continue
        expected = current[path]
        target = os.readlink(path)
        if target not in (expected['target'], entry['target']): raise ValueError('launcher manually changed; refusing rollback')
        wanted = expected['sha256'] if target == expected['target'] else entry['sha256']
        if digest(path) != wanted: raise ValueError('launcher binary changed; refusing rollback')
        if digest(Path(path).parent / entry['target']) != entry['sha256']: raise ValueError('original binary changed')
    for entry in record['data']:
        if digest(directory / entry['copy']) != entry['sha256']: raise ValueError('data backup changed')
        if restore_data is not None and digest(entry['path']) != restore_data[entry['path']]: raise ValueError('data changed since restore approval')
    for entry in record['links']:
        if entry['path'] in current: atomic_link(entry['path'], entry['target'])
    if restore_data is not None:
        for entry in record['data']:
            path = Path(entry['path']); temporary = path.with_name('.restore-' + uuid.uuid4().hex)
            try:
                with temporary.open('xb') as output:
                    output.write((directory / entry['copy']).read_bytes()); output.flush(); os.fsync(output.fileno())
                temporary.chmod(entry['mode']); os.replace(temporary, path)
            finally:
                if temporary.exists(): temporary.unlink()
    return record

if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('backup', type=Path)
    args = parser.parse_args()
    rollback(args.backup)
    print('Launcher rollback verified; data left untouched')
