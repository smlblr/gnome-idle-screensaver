#!/usr/bin/python3
"""Synthetic input through Mutter's RemoteDesktop API. Args: move:dx,dy  key:0xffe1  sleep:s"""
import sys, time
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
RD = 'org.gnome.Mutter.RemoteDesktop'
SESSION = RD + '.Session'
def call(path, iface, method, args=None, reply=None):
    return bus.call_sync(RD, path, iface, method, args, GLib.VariantType(reply) if reply else None, 0, 5000, None)
path = call('/org/gnome/Mutter/RemoteDesktop', RD, 'CreateSession', None, '(o)').unpack()[0]
call(path, SESSION, 'Start')
time.sleep(0.3)
for action in sys.argv[1:]:
    kind, _, arg = action.partition(':')
    if kind == 'sleep':
        time.sleep(float(arg)); continue
    t = time.monotonic()
    if kind == 'move':
        dx, dy = map(float, arg.split(','))
        call(path, SESSION, 'NotifyPointerMotionRelative', GLib.Variant('(dd)', (dx, dy)))
    elif kind == 'key':
        keysym = int(arg, 0)
        call(path, SESSION, 'NotifyKeyboardKeysym', GLib.Variant('(ub)', (keysym, True)))
        call(path, SESSION, 'NotifyKeyboardKeysym', GLib.Variant('(ub)', (keysym, False)))
    print(f'[{t:.3f}] injected {action}', flush=True)
time.sleep(0.2)
call(path, SESSION, 'Stop')
