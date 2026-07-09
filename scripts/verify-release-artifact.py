import argparse
import pathlib
import sys
import tarfile
import tempfile
import zipfile


def verify(artifact_path: pathlib.Path, platform_kind: str) -> None:
    if not artifact_path.exists():
        raise RuntimeError(f"artifact not found: {artifact_path}")

    with tempfile.TemporaryDirectory(prefix="muldex-artifact-check-") as temp_root:
        temp_root_path = pathlib.Path(temp_root)

        if artifact_path.name.endswith('.zip'):
            with zipfile.ZipFile(artifact_path, 'r') as archive:
                archive.extractall(temp_root_path)
        elif artifact_path.name.endswith('.tar.gz'):
            with tarfile.open(artifact_path, 'r:gz') as archive:
                archive.extractall(temp_root_path)
        else:
            raise RuntimeError(f"unsupported artifact extension: {artifact_path}")

        names = {path.name for path in temp_root_path.rglob('*') if path.is_file()}

        if platform_kind == 'windows':
            required = {'muldex.exe', 'install.ps1', 'uninstall.ps1', 'README.txt'}
        else:
            required = {'muldex', 'install.sh', 'uninstall.sh', 'README.txt'}

        missing = sorted(required - names)
        if missing:
            raise RuntimeError(f"missing artifact entries: {', '.join(missing)}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--artifact-path', required=True)
    parser.add_argument('--platform-kind', required=True, choices=['windows', 'unix'])
    args = parser.parse_args()

    try:
        verify(pathlib.Path(args.artifact_path), args.platform_kind)
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 1

    print('artifact.verify: ok')
    print(f'artifact.path: {args.artifact_path}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
