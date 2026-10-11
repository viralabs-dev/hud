#!/usr/bin/env python3
"""Explicit, offline one-shot geometry import. Python is not a Rust runtime dependency."""
import argparse
import json
import os
from pathlib import Path
import stat
import sys
import tomllib

# Installed location: repository/rust/scripts/import_python_layout.py.
ROOT = next((p for p in Path(__file__).resolve().parents if (p / 'hud/layout.py').is_file()), None)
if ROOT is None:
    raise RuntimeError('Import bridge requires the HUD checkout; no personal path fallback')
sys.path.insert(0, str(ROOT))
from hud import layout as legacy

MAPPING = {'sistema': 'system', 'saida': 'output'}
LIMIT = 64 * 1024


def pinned_parent(path):
    """POSIX only; reject traversal and symlink components, including ancestors."""
    path = Path(path)
    if not path.is_absolute() or '..' in path.parts or path == Path('/'):
        raise ValueError('explicit absolute path without parent traversal required')
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
    fd = os.open('/', flags)
    try:
        for part in path.parts[1:-1]:
            nxt = os.open(part, flags, dir_fd=fd)
            os.close(fd)
            fd = nxt
            st = os.fstat(fd)
            if st.st_mode & 0o022 and not st.st_mode & stat.S_ISVTX:
                raise ValueError('writable non-sticky ancestor refused')
        return fd, path.name
    except BaseException:
        os.close(fd)
        raise


def read_source(path):
    parent, name = pinned_parent(path)
    try:
        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=parent)
        with os.fdopen(fd, 'rb') as stream:
            st = os.fstat(stream.fileno())
            if st.st_uid != os.getuid() or st.st_mode & 0o022:
                raise ValueError('source must be owned and not writable by other users')
            if not stat.S_ISREG(st.st_mode) or st.st_size > LIMIT:
                raise ValueError('source must be bounded regular file')
            data = stream.read(LIMIT + 1)
            if len(data) > LIMIT:
                raise ValueError('source grew beyond limit')
            return data
    finally:
        os.close(parent)


def preview(data):
    if len(data) > LIMIT:
        raise ValueError('source exceeds 64 KiB')
    raw = tomllib.loads(data.decode('utf-8'))
    if set(raw) - {'nome', 'descricao', 'autor', 'coluna', 'painel', 'bloco'}:
        raise ValueError('unknown top-level keys refused')
    # Custom commands/files must never be partially migrated or resolved/executed.
    if raw.get('painel') or raw.get('bloco'):
        raise ValueError('custom panels and block overrides require later parity; import refused')
    lay = legacy.parse(raw, str(raw.get('nome', 'imported')), None)
    columns = [[MAPPING.get(slot.id, slot.id) for slot in col.slots] for col in lay.colunas]
    flat = [p for c in columns for p in c]
    if len(flat) != len(set(flat)):
        raise ValueError('mapped IDs collide')
    text = 'HUD-LAYOUT 1\n' + ''.join(','.join(c) + '\n' for c in columns)
    return text.encode(), {
        'columns': columns,
        'warnings': ['Order and IDs only: widths, heights, weights and metadata are not represented in Rust v1.',
                     'Imported IDs do not imply functional panel parity.'],
        'mapping': MAPPING,
    }


def commit(target, original, encoded):
    """New Rust directory only. Backup precedes state; never overwrite any existing destination."""
    parent, name = pinned_parent(target)
    directory = None
    try:
        os.mkdir(name, 0o700, dir_fd=parent)  # EXCLUSIVE; existing state, links, dirs refused
        directory = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
        st = os.fstat(directory)
        if st.st_uid != os.getuid() or st.st_mode & 0o077:
            raise ValueError('destination must be private and owned')
        def exclusive_write(filename, content):
            fd = os.open(filename, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
            with os.fdopen(fd, 'wb') as stream:
                stream.write(content)
                stream.flush()
                os.fsync(stream.fileno())
        exclusive_write('python-layout.original.toml', original)
        os.fsync(directory)  # durable backup name precedes state creation
        os.fsync(parent)  # durable newly created directory precedes state
        temporary = '.layout.import.tmp'
        try:
            exclusive_write(temporary, encoded)
            os.rename(temporary, 'layout.state', src_dir_fd=directory, dst_dir_fd=directory)
            os.fsync(directory)
        except BaseException:
            try:
                os.unlink(temporary, dir_fd=directory)
            except FileNotFoundError:
                pass
            raise
        os.fsync(parent)
    except BaseException:
        # Keep successfully written backup on failure for explicit manual recovery.
        # No Python source or preexisting target is ever modified.
        raise
    finally:
        if directory is not None:
            os.close(directory)
        os.close(parent)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--target', type=Path)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--accept-geometry-loss', action='store_true')
    args = parser.parse_args(argv)
    if os.name != 'posix':
        parser.error('secure import currently supports POSIX only')
    try:
        data = read_source(args.source)
        encoded, report = preview(data)
        if args.apply:
            if not args.target or not args.accept_geometry_loss:
                raise ValueError('--apply requires --target and --accept-geometry-loss')
            commit(args.target, data, encoded)
        report['applied'] = args.apply
        report['encoded_bytes'] = len(encoded)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, legacy.LayoutError) as error:
        print(f'import refused: {error}', file=sys.stderr)
        return 2

if __name__ == '__main__':
    raise SystemExit(main())
