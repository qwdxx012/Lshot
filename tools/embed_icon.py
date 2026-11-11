"""Embed an .ico into a PE executable using the Win32 UpdateResource API.

Needed for GNU-targeted local builds where no resource compiler (windres/rc.exe)
is available; CI builds already embed the icon through build.rs.
"""
import ctypes
import struct
import sys
from ctypes import wintypes

RT_ICON = 3
RT_GROUP_ICON = 14

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.BeginUpdateResourceW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL]
kernel32.BeginUpdateResourceW.restype = wintypes.HANDLE
kernel32.UpdateResourceW.argtypes = [
    wintypes.HANDLE,
    ctypes.c_void_p,
    ctypes.c_void_p,
    wintypes.WORD,
    ctypes.c_void_p,
    wintypes.DWORD,
]
kernel32.UpdateResourceW.restype = wintypes.BOOL
kernel32.EndUpdateResourceW.argtypes = [wintypes.HANDLE, wintypes.BOOL]
kernel32.EndUpdateResourceW.restype = wintypes.BOOL

IMAGE_RESOURCE_ID = ctypes.c_void_p


def load_ico(path):
    with open(path, "rb") as handle:
        data = handle.read()
    reserved, kind, count = struct.unpack_from("<HHH", data, 0)
    if reserved != 0 or kind != 1:
        raise ValueError("not an icon file")
    images = []
    for index in range(count):
        width, height, colors, _, planes, bits, size, offset = struct.unpack_from(
            "<BBBBHHII", data, 6 + index * 16
        )
        images.append(
            (width, height, colors, planes, bits, data[offset : offset + size])
        )
    return images


def group_icon(images):
    header = struct.pack("<HHH", 0, 1, len(images))
    entries = b"".join(
        struct.pack(
            "<BBBBHHIH",
            width,
            height,
            colors,
            0,
            planes,
            bits,
            len(blob),
            index + 1,
        )
        for index, (width, height, colors, planes, bits, blob) in enumerate(images)
    )
    return header + entries


def embed(exe_path, ico_path):
    images = load_ico(ico_path)
    update = kernel32.BeginUpdateResourceW(exe_path, False)
    if not update:
        raise OSError(ctypes.get_last_error(), "BeginUpdateResourceW failed")
    try:
        for index, (_, _, _, _, _, blob) in enumerate(images):
            buffer = ctypes.create_string_buffer(blob, len(blob))
            if not kernel32.UpdateResourceW(
                update,
                IMAGE_RESOURCE_ID(RT_ICON),
                IMAGE_RESOURCE_ID(index + 1),
                0,
                buffer,
                len(blob),
            ):
                raise OSError(ctypes.get_last_error(), "UpdateResource(RT_ICON) failed")
        blob = group_icon(images)
        buffer = ctypes.create_string_buffer(blob, len(blob))
        if not kernel32.UpdateResourceW(
            update,
            IMAGE_RESOURCE_ID(RT_GROUP_ICON),
            IMAGE_RESOURCE_ID(1),
            0,
            buffer,
            len(blob),
        ):
            raise OSError(ctypes.get_last_error(), "UpdateResource(RT_GROUP_ICON) failed")
    except BaseException:
        kernel32.EndUpdateResourceW(update, True)
        raise
    if not kernel32.EndUpdateResourceW(update, False):
        raise OSError(ctypes.get_last_error(), "EndUpdateResourceW failed")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: embed_icon.py <exe> <ico>")
    embed(sys.argv[1], sys.argv[2])
    print("embedded", sys.argv[2], "into", sys.argv[1])
