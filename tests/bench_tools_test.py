"""Bench tooling contracts without opening serial ports or driving hardware."""
import contextlib
import hashlib
import hmac
import io
import json
from pathlib import Path
import stat
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT / 'scripts'))
from bench_console import encode, calibrate, execute
from esp32_firmware import init_profile, flash_bench

VECTOR='4145524f02013c00887766554433221105000000c80070fe2c01e80325d96b6961c07ed0131f2a6fd92374196111bd58d876103d06627c6b32958c64'

class ToolsTests(unittest.TestCase):
    def test_packet_matches_rust_vector_and_standard_hmac(self):
        p=encode(bytes([0x42]*32),0x1122334455667788,5,arm=True,throttle=.2,roll=-.4,pitch=.3,yaw=1.)
        self.assertEqual(p.hex(),VECTOR)
        self.assertEqual(len(p),60)
        self.assertEqual(p[28:],hmac.digest(bytes([0x42]*32),p[:28],hashlib.sha256))
        for values in [dict(throttle=float('nan')),dict(roll=1.001),dict(throttle=-.01),dict(emergency=True,arm=True)]:
            with self.assertRaises(ValueError):encode(bytes([0x42]*32),1,1,**values)

    def test_private_profile_is_unique_exclusive_and_uncalibrated(self):
        with tempfile.TemporaryDirectory() as d,contextlib.redirect_stdout(io.StringIO()):
            a,b=Path(d)/'a.json',Path(d)/'b.json'
            init_profile(a);init_profile(b)
            one,two=json.loads(a.read_text()),json.loads(b.read_text())
            self.assertNotEqual(one['psk_hex'],two['psk_hex']);self.assertNotEqual(one['wifi_password'],two['wifi_password'])
            self.assertIsNone(one['battery_calibration']);self.assertEqual(one['max_pwm_us'],1150)
            self.assertEqual(stat.S_IMODE(a.stat().st_mode),0o600)
            with self.assertRaises(FileExistsError):init_profile(a)
            calibrate(a,2400,9.,3360,12.6)
            self.assertEqual(json.loads(a.read_text())['battery_calibration']['raw_low'],2400)
            with self.assertRaises(ValueError):calibrate(a,0,9.,4095,12.6)
            self.assertEqual(json.loads(a.read_text())['psk_hex'],one['psk_hex'])

    def test_emulated_image_is_never_passed_to_flash_tool(self):
        with tempfile.TemporaryDirectory() as d,patch('esp32_firmware.run') as run:
            folder=Path(d);(folder/'manifest.json').write_text(json.dumps(dict(milestone=2,mode='emulated')))
            with self.assertRaises(SystemExit):flash_bench('/dev/unused',folder)
            run.assert_not_called()

    def test_client_disarms_when_stream_raises_and_does_not_arm_for_status(self):
        class Transport:
            packets=[]
            def status(self):return dict(session='1122334455667788',next_sequence=5)
            def send(self,p):self.packets.append(p)
        t=Transport()
        with contextlib.redirect_stdout(io.StringIO()):execute(t,bytes([0x42]*32),'status',1)
        self.assertEqual(t.packets,[])
        with patch('bench_console.time.sleep',side_effect=[KeyboardInterrupt,None,None,None]),contextlib.redirect_stdout(io.StringIO()),self.assertRaises(KeyboardInterrupt):
            execute(t,bytes([0x42]*32),'arm',1)
        self.assertEqual([p[5] for p in t.packets],[1,0,0,0])
        self.assertEqual([struct.unpack_from('<I',p,16)[0] for p in t.packets],[5,6,7,8])

if __name__=='__main__':unittest.main()
