import hashlib
from pathlib import Path
import struct
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_esp32_image import verify

class ImageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder=ROOT / "output/firmware/esp32-devkitc-m1"
        cls.elf=(folder / "aerolab-esp32-devkitc.elf").read_bytes()
        cls.image=(folder / "aerolab-esp32-devkitc-merged.bin").read_bytes()

    def test_packaged_firmware_has_exact_board_and_partition_layout(self):
        result=verify(self.elf,self.image)
        self.assertEqual(result["chip"],"esp32")
        self.assertEqual(result["flash_bytes"],4*1024*1024)
        self.assertEqual(result["application_offset"],"0x10000")
        self.assertFalse(result["hardware_boot_test_included"])

    def test_truncated_corrupt_wrong_chip_and_mismatched_entry_are_rejected(self):
        for data in [self.image[:0x10000],self.image[:-40]]:
            with self.assertRaises(ValueError): verify(self.elf,data)
        for position in [0x1000,0x100c,0x1028,0x8010,0x10030]:
            image=bytearray(self.image);image[position]^=1
            with self.subTest(position=position),self.assertRaises(ValueError): verify(self.elf,image)
        elf=bytearray(self.elf);elf[24]^=1
        with self.assertRaisesRegex(ValueError,"entry"):verify(elf,self.image)

    def test_overlapping_partitions_are_rejected_even_with_recomputed_md5(self):
        image=bytearray(self.image)
        struct.pack_into("<I",image,0x8000+4,0x10000)
        checksum=next(n for n in range(0x8000,0x9000,32) if image[n:n+2]==b"\xeb\xeb")
        image[checksum+16:checksum+32]=hashlib.md5(image[0x8000:checksum]).digest()
        with self.assertRaisesRegex(ValueError,"Overlapping"):verify(self.elf,image)

if __name__=="__main__":unittest.main()
