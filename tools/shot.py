#!/usr/bin/python3
"""Non-interactive portal screenshot, moved out of ~/Pictures to argv[1]."""
import os, shutil, sys
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
token = 'gis%d' % os.getpid()
sender = bus.get_unique_name()[1:].replace('.', '_')
request = f'/org/freedesktop/portal/desktop/request/{sender}/{token}'
loop, result = GLib.MainLoop(), {}
def on_response(conn, snd, path, iface, sig, params, *a):
    code, res = params.unpack(); result.update(code=code, uri=res.get('uri')); loop.quit()
bus.signal_subscribe('org.freedesktop.portal.Desktop', 'org.freedesktop.portal.Request', 'Response', request, None, 0, on_response)
opts = {'handle_token': GLib.Variant('s', token), 'interactive': GLib.Variant('b', False)}
bus.call_sync('org.freedesktop.portal.Desktop', '/org/freedesktop/portal/desktop', 'org.freedesktop.portal.Screenshot',
              'Screenshot', GLib.Variant('(sa{sv})', ('', opts)), None, 0, 10000, None)
GLib.timeout_add(10000, loop.quit); loop.run()
if result.get('code') != 0 or not result.get('uri'):
    sys.exit(f'screenshot failed: {result}')
shutil.move(Gio.File.new_for_uri(result['uri']).get_path(), sys.argv[1])
print('saved', sys.argv[1])
