Short answer: yes each OS exposes system-level idle/screen state APIs you can use to detect user inactivity or screen lock and then run nice (or similar) to change priority.

Links and APIs by OS:

- Windows GetLastInputInfo for idle time; WTSRegisterSessionNotification / WTS_SESSION_LOCK and WM_WTSSESSION_CHANGE for lock/unlock; SystemParametersInfo(SPI_GETSCREENSAVEACTIVE) and RegisterPowerSettingNotification for display/power events. Microsoft docs: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getlastinputinfo and https://learn.microsoft.com/windows/win32/termserv/wtsregistersessionnotification

- Linux (X11) XScreenSaverQueryInfo (X11 XScreenSaver extension) returns idle ms; DBus interfaces from screensaver/power managers (e.g., org.gnome.Mutter.IdleMonitor, org.freedesktop.ScreenSaver) or logind via systemd-logind (Inhibit/IdleHint). Examples: https://www.x.org/releases/current/doc/man/man3/XScreenSaverQueryInfo.3.xhtml and https://www.freedesktop.org/wiki/Software/systemd/logind/

- Linux (Wayland/desktop-specific) No single Wayland protocol for idle; use compositor/desktop D-Bus APIs (GNOME IdleMonitor, KDE org.freedesktop.ScreenSaver, wlroots backend-specific hooks). GNOME docs: https://gjs-docs.gnome.org/overrides/Gio.DBusProxy.html (search IdleMonitor)

- macOS  IOKit IOPowerSources/IOPMAssertion + IORegisterForSystemPower for power/display sleep; IOHIDGetActivity or CGEventSourceSecondsSinceLastEventType / IOKit IOPMConnection/IOPMCopyAssertion to detect idle/lock. Apple docs: https://developer.apple.com/documentation/iokit and https://developer.apple.com/documentation/coregraphics/1454760-cgeventsourcesecondssincelasteventtype

- Cross-platform / higher-level options  Use DBus for Linux desktops, platform bindings/wrappers (libxss for X11), or existing tools: xscreensaver's XSS API, xprintidle, or uptime-style utilities. For browsers/web, the experimental Idle Detection API (not for OS-level/background daemons): https://developer.mozilla.org/docs/Web/API/Idle_Detection_API

Notes: on Wayland there's no universal protocol  rely on compositor-specific APIs. Prefer lock/ session notifications (WTS on Windows, logind/ScreenSaver D-Bus on Linux, IOKit on macOS) rather than polling input if you need robust detection.

If you want, tell me which OS and language and I'll give the exact function/event names and links for that stack.
