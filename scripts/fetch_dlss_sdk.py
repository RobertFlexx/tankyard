#!/usr/bin/env python3
"""Fetch the pinned NVIDIA DLSS SDK pieces the client links against.

The SDK is NVIDIA's (see its LICENSE.txt), so it is downloaded at build time
rather than kept in this repository. Every file is checked against a pinned
SHA-256; a mismatch stops the build. Without these files the client still
builds, just without DLSS.

    python3 scripts/fetch_dlss_sdk.py [destination]   # default: target/dlss-sdk

The DLSS runtime itself (about 60 MB) is not fetched here: the game downloads
it on first use from the same pinned SDK release and checks the same way.
"""
import hashlib
import io
import os
import sys
import tarfile
import urllib.request
from pathlib import Path

DLSS_COMMIT = '9981c6350dc58c0ce21f024fd65e31e851c31689'  # tag v310.9.16
DLSS_FILES = {
    'include/nvsdk_ngx.h': 'dc38e7467cf415379c9d12ae1b6e4a494c453ed92720fb53e92aecb523e7b848',
    'include/nvsdk_ngx_defs.h': 'ea23f33497cd274860d1c25a97644fce807dcb0037c594547203343103fad03e',
    'include/nvsdk_ngx_defs_vk.h': '0a24d0861ace7d6b9362a67b7f08bea1b33ea123c5ce730b19133de8a891d031',
    'include/nvsdk_ngx_vk.h': '2d364ce7132881eb669e9498fd570d74cba563b1fbebc82c235f8e1ad2dd8b6d',
    'include/nvsdk_ngx_params.h': '943bc8cc5cdae03b6303016fbad3183636f2335ae27a2d18776798c3b4efabbc',
    'include/nvsdk_ngx_helpers.h': '5bcbadfe7478b802cf6d3aca4dc5ddd7d0889b99726e69c63f9e9bd555f44471',
    'include/nvsdk_ngx_helpers_vk.h': 'c192bff72138f12f770db48e12c1d8f712dfc81ea0e8e055ebb27d8dd1b31623',
    'include/nvsdk_ngx_helpers_d3d.h': 'ec75224f36ed6580baaf250fa83620405ad81a1d98c2c436654a0c9cf6a6b8ba',
    'include/nvsdk_ngx_helpers_cuda.h': 'f2851a76107bdf4fbcb7b261f3c35573c312d8ecb4bc18fb02d9e2cec7705042',
    'include/nvsdk_ngx_loader.h': 'abdf5f66a692620538357d29fdf78e3e2a677ee8140b7a638962ecb766585e75',
    'include/nvsdk_ngx_standalone_cuda.h': 'b23436ed5040051ac8e81554c9af080f2463c941435bbc3b82222e5431a09499',
    'lib/Linux_x86_64/libnvsdk_ngx.a': 'd3c50bb0d9624fc35e502a8e929b8fc1e2b505017ebdb84c0d5b627e11a235df',
    'lib/Windows_x86_64/x64/nvsdk_ngx_d.lib': 'bc43df9ec032004fde681d3e3afd01cde239e03a0125b822e88674c84287f99c',
    'lib/Windows_x86_64/x64/nvsdk_ngx_s.lib': '7b9a5239c1145af5f645ab472aa066f9ea79db36ac819562312aae69d65f3765',
    'LICENSE.txt': 'd4216e39ebef5f9b50a6712ebb37beeb5379862a67733a9999c651f21592aaf0',
}
VULKAN_HEADERS = (
    'https://codeload.github.com/KhronosGroup/Vulkan-Headers/tar.gz/refs/tags/v1.3.296',
    'e204e0b3c19f622d197df945737f5db913d6621830999b8578d34e80a7c90585',
)


def download(url: str, digest: str) -> bytes:
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read()
    actual = hashlib.sha256(data).hexdigest()
    if actual != digest:
        sys.exit(f'{url}: SHA-256 {actual} does not match the pinned {digest}')
    return data


def write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.partial')
    temporary.write_bytes(data)
    os.replace(temporary, path)


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    destination = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'target' / 'dlss-sdk'
    for name, digest in DLSS_FILES.items():
        path = destination / name
        if path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == digest:
            continue
        url = f'https://raw.githubusercontent.com/NVIDIA/DLSS/{DLSS_COMMIT}/{name}'
        write(path, download(url, digest))
    marker = destination / 'vulkan' / '.v1.3.296'
    if not marker.is_file():
        archive = tarfile.open(fileobj=io.BytesIO(download(*VULKAN_HEADERS)))
        for member in archive.getmembers():
            parts = Path(member.name).parts
            # Only the C headers: include/vulkan and include/vk_video.
            if (member.isfile() and len(parts) == 4 and parts[1] == 'include'
                    and parts[2] in ('vulkan', 'vk_video') and parts[3].endswith('.h')):
                write(destination / 'vulkan' / 'include' / parts[2] / parts[3],
                      archive.extractfile(member).read())
        write(marker, b'')
    print(f'DLSS SDK ready in {destination}')


if __name__ == '__main__':
    main()
