# log fcitx5's virtual keyboard show/hide requests (test helper)
import time
from gi.repository import Gio, GLib
XML = """<node><interface name="org.fcitx.Fcitx5.VirtualKeyboard1">
<method name="ShowVirtualKeyboard"/><method name="HideVirtualKeyboard"/>
</interface></node>"""
def call(conn, sender, path, iface, method, params, inv):
    print(f"{time.time():.2f} {method}", flush=True)
    inv.return_value(None)
def acquired(conn, name):
    conn.register_object("/org/fcitx/virtualkeyboard/impanel", Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], call)
Gio.bus_own_name(Gio.BusType.SESSION, "org.fcitx.Fcitx5.VirtualKeyboard", 0, acquired, None, None)
GLib.MainLoop().run()
