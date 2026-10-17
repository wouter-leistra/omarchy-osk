#!/usr/bin/python3 -I
"""Boot the running kernel in QEMU with an initramfs containing the
omarchy-osk-unlock hook and a throwaway LUKS disk, then drive unl0kr.

usage: tests/unlock_vm.py build
       tests/unlock_vm.py run SCENARIO [SCENARIO...]

Scenarios: keyboard, wrong-then-right, touch, cover, fallback, skip. Everything goes
to /tmp/osk-vm (screenshots, serial logs). Needs qemu-base and unlock/build.sh.
"""

import json
import os
import shutil
import socket
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
UNLOCK = os.path.join(os.path.dirname(HERE), "unlock")
OUT = "/tmp/osk-vm"
KVER = os.uname().release
PASSWORD = "osk test"
# OSK_VM_SIZE=2160x1440 to check the look at the Surface's resolution
# (the touch scenario's key positions are for 1440x960)
W, H = (int(v) for v in os.environ.get("OSK_VM_SIZE", "1440x960").split("x"))


def sh(*cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def build():
    """test initramfs: the live mkinitcpio config, no autodetect, plus VM drivers"""
    shutil.rmtree(OUT, ignore_errors=True)
    for d in ("initcpio/install", "initcpio/hooks"):
        os.makedirs(os.path.join(OUT, d))
    install = open(os.path.join(UNLOCK, "install")).read()
    install = install.replace("    add_runscript", f'    add_file "{UNLOCK}/unl0kr.conf" /etc/unl0kr.conf\n    add_runscript')
    open(os.path.join(OUT, "initcpio/install/omarchy-osk-unlock"), "w").write(install)
    shutil.copy(os.path.join(UNLOCK, "hook"), os.path.join(OUT, "initcpio/hooks/omarchy-osk-unlock"))

    conf = open("/etc/mkinitcpio.conf").read()
    dropins = sorted(os.listdir("/etc/mkinitcpio.conf.d"))
    dropins = [os.path.join("/etc/mkinitcpio.conf.d", f) for f in dropins if f.endswith(".conf")
               and f != "omarchy_osk_unlock.conf"]
    for f in dropins:
        conf += "\n" + open(f).read()
        if os.path.basename(f) == "omarchy_hooks.conf":
            conf += "\n" + open(os.path.join(UNLOCK, "mkinitcpio.conf")).read()
    conf += """
_vm=()
for _h in "${HOOKS[@]}"; do [[ $_h == autodetect ]] || _vm+=("$_h"); done
HOOKS=("${_vm[@]}")
MODULES+=(bochs virtio_pci virtio_blk virtio_input)
"""
    open(os.path.join(OUT, "mkinitcpio.conf"), "w").write(conf)
    # the Omarchy look from this user's theme, at the VM's screen size
    env = dict(os.environ, OSK_UNL0KR=os.path.join(UNLOCK, "build/unl0kr"),
               OSK_LOOK=os.path.join(UNLOCK, "omarchy-look"), OSK_USER=os.environ["USER"],
               OSK_HOME=os.path.expanduser("~"), OSK_SCREEN=f"{W}x{H}",
               # the VM's keyboard plays the Type Cover
               OSK_COVER_NAME="Virtio Keyboard")
    r = subprocess.run(["/usr/bin/mkinitcpio", "-c", f"{OUT}/mkinitcpio.conf",
                        "-D", f"{OUT}/initcpio", "-D", "/usr/lib/initcpio",
                        "-k", KVER, "-g", f"{OUT}/initramfs.img"],
                       env=env, capture_output=True, text=True)
    print("\n".join(l for l in r.stdout.splitlines() if "ERROR" in l or "osk" in l or "successful" in l))
    if r.returncode != 0:
        sys.exit(r.stderr)

    disk = f"{OUT}/luks.img"
    with open(disk, "wb") as f:
        f.truncate(64 << 20)
    sh("cryptsetup", "luksFormat", "--type", "luks2", "--batch-mode", "--pbkdf", "pbkdf2",
       "--pbkdf-force-iterations", "1000", "--key-file=-", disk, input=PASSWORD.encode())


class VM:
    def __init__(self, name, extra=""):
        self.name = name
        self.serial = f"{OUT}/{name}.serial"
        sock = f"{OUT}/{name}.qmp"
        if os.path.exists(sock):
            os.remove(sock)
        cmdline = ("cryptdevice=/dev/vda:root root=/dev/mapper/root rw quiet splash "
                   "plymouth.ignore-serial-consoles console=tty0 console=ttyS0 loglevel=6 " + extra)
        self.proc = subprocess.Popen([
            "qemu-system-x86_64", "-enable-kvm", "-m", "1536", "-smp", "2",
            "-kernel", f"/usr/lib/modules/{KVER}/vmlinuz", "-initrd", f"{OUT}/initramfs.img",
            "-append", cmdline, "-display", "none",
            "-device", f"VGA,xres={W},yres={H},edid=on",
            "-device", "virtio-tablet-pci", "-device", "virtio-keyboard-pci,id=kbd",
            "-drive", f"file={OUT}/luks.img,format=raw,if=virtio,snapshot=on",
            "-serial", f"file:{self.serial}", "-qmp", f"unix:{sock},server=on,wait=off",
            "-no-reboot"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(50):
            if os.path.exists(sock):
                break
            time.sleep(0.1)
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(sock)
        self.f = self.s.makefile("rw")
        self.f.readline()
        self.qmp("qmp_capabilities")

    def qmp(self, cmd, **args):
        self.f.write(json.dumps({"execute": cmd, "arguments": args}) + "\n")
        self.f.flush()
        while True:
            r = json.loads(self.f.readline())
            if "return" in r or "error" in r:
                return r

    def shot(self, tag):
        path = f"{OUT}/{self.name}-{tag}.png"
        self.qmp("screendump", filename=path, format="png")
        return path

    def log(self):
        return open(self.serial, errors="replace").read()

    def wait_log(self, text, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            if text in self.log():
                return True
            time.sleep(0.5)
        return False

    def type(self, text, enter=True):
        names = {" ": "spc"}
        for c in text:
            self.qmp("send-key", keys=[{"type": "qcode", "data": names.get(c, c)}])
            time.sleep(0.08)
        if enter:
            self.qmp("send-key", keys=[{"type": "qcode", "data": "ret"}])

    def tap(self, x, y):
        ax, ay = int(x * 32767 / W), int(y * 32767 / H)
        self.qmp("input-send-event", events=[
            {"type": "abs", "data": {"axis": "x", "value": ax}},
            {"type": "abs", "data": {"axis": "y", "value": ay}}])
        time.sleep(0.05)
        self.qmp("input-send-event", events=[{"type": "btn", "data": {"down": True, "button": "left"}}])
        time.sleep(0.06)
        self.qmp("input-send-event", events=[{"type": "btn", "data": {"down": False, "button": "left"}}])
        time.sleep(0.15)

    def cover(self, attached):
        if attached:
            self.qmp("device_add", driver="virtio-keyboard-pci", id="kbd")
        else:
            self.qmp("device_del", id="kbd")
        time.sleep(2)

    def close(self):
        self.proc.kill()
        self.proc.wait()


# unlocked by our hook (its kernel log line)
OURS = "omarchy-osk-unlock: unlocked root"
# unlocked either way: the (empty) root then fails to mount
OPENED = "/dev/mapper/root"


def wait_for_unl0kr(vm, seconds=20):
    time.sleep(seconds)
    return vm.shot("prompt")


def scenario(name):
    vm = VM(name, "oskunlock=0" if name == "skip" else "")
    ok = False
    try:
        if name == "keyboard":
            wait_for_unl0kr(vm)
            vm.type(PASSWORD)
            ok = vm.wait_log(OURS)
        elif name == "wrong-then-right":
            wait_for_unl0kr(vm)
            vm.type("nope")
            time.sleep(4)
            vm.shot("retry")
            vm.type(PASSWORD)
            ok = vm.wait_log(OURS)
        elif name == "touch":
            wait_for_unl0kr(vm)
            vm.cover(False)      # the on-screen keyboard shows without the cover
            from unlock_vm_keys import KEYS    # positions measured from a screenshot
            for c in PASSWORD:
                vm.tap(*KEYS[c])
            vm.shot("typed")
            vm.tap(*KEYS["\n"])
            ok = vm.wait_log(OURS)
        elif name == "cover":
            # keyboard hidden with the cover, shown without, hidden again
            wait_for_unl0kr(vm)
            vm.cover(False)
            vm.shot("detached")
            vm.cover(True)
            vm.shot("attached")
            vm.type(PASSWORD)
            ok = vm.wait_log(OURS)
        elif name == "fallback":
            # three wrong passphrases: unl0kr gives up, Plymouth asks
            wait_for_unl0kr(vm)
            for _ in range(3):
                vm.type("nope")
                time.sleep(4)
            vm.shot("plymouth")
            vm.type(PASSWORD)
            time.sleep(6)
            ok = OURS not in vm.log() and vm.wait_log(OPENED)
        elif name == "skip":
            time.sleep(15)
            vm.shot("plymouth")
            vm.type(PASSWORD)
            time.sleep(6)
            ok = OURS not in vm.log() and vm.wait_log(OPENED)
        vm.shot("end")
    finally:
        vm.close()
    print(f"{name}: {'PASS' if ok else 'FAIL'}  (log {vm.serial})", flush=True)
    return ok


def main():
    if sys.argv[1] == "build":
        build()
    else:
        sys.path.insert(0, HERE)
        results = [scenario(n) for n in sys.argv[2:]]
        sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()
