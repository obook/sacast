# Roadmap

sacast started from sbcast: screen capture, menu and session are shared,
the receiver protocol is new.

## Receiver protocol

Done:

- [x] Find receivers with mDNS (name, id, model)
- [x] Stub sender: runs the capture and drops the frames, so the menu's stats work without a receiver
- [x] Local test receiver: UxPlay (`uxplay -n Test -p`), open access
- [x] Read the receiver's info (`GET /info`): `sacast info`, and logged at each cast. Checked on UxPlay
- [x] When a receiver announces several addresses, pick the one on the PC's network (UxPlay also announced the Docker bridge address)

To do, in order:

- [ ] `sacast info` on an Apple TV at school: model, version, status flags (password)
- [ ] PIN pairing: SRP-6a with a PIN shown on the receiver, then an Ed25519 / X25519 verify step; keep the keys per receiver
- [ ] Access modes an Apple TV can be set to, by hand or by device management (AirPlay Security payload): first-time passcode, passcode each time, or a known password shared by all the devices (the school's case: the password is known, it stays out of the repo). Find how each one shows in `/info` or the mDNS record, and which step takes the password (hypothesis: the pairing step, in place of the PIN). Ask for it in the menu and with `--password`
- [ ] "Same Wi-Fi network only" access: check that it does not block a wired PC
- [ ] Check whether the keys from the verify step are enough to encrypt the streams, before any work on the legacy key exchange
- [ ] Legacy key exchange, only if the check above fails. Main risk: no public specification
- [ ] Session setup: control, timing and data ports
- [ ] Video: H.264 frames, encrypted, over TCP
- [ ] Clock sync for audio and video
- [ ] Sound: AAC-ELD or ALAC over RTP
- [ ] Reconnect after a loss; report reconnections and network drops to the menu
- [ ] PIN entry in the menu and on the command line
- [ ] Password, as at school: choose an Apple TV in the list, type its password once in a dialog, then never again for that Apple TV unless the password changes (ask again when it is refused). Keep one password per Apple TV id in `~/.config/sacast/`, readable by the user only; the desktop keyring (KWallet) could replace the file later
- [ ] Test on a real receiver

## Menu

Done:

- [x] Audio delay in 50 ms steps, from -500 to 1500 ms (negative: the image waits), applied at the next cast: changed live, it froze the image in sccast
- [x] Name a missing GStreamer element and its package at start

To do:

- [ ] Tune the alert thresholds on a real receiver and the school network
- [ ] `f` freezes the image, as in sbcast and sccast (obook/sbcast#11): the receiver keeps the current frame with "Freeze" in its top right corner, `f` again goes live. Port sbcast's overlay and frame probe once video reaches a receiver

## Packaging

Done:

- [x] First release, v0.1.0 (preview: list and info, no cast yet): .deb for KDE neon and Ubuntu 26.04

To do:

- [ ] Cast for one hour without a cut

## Links

- [Apple Platform Deployment: AirPlay Security payload settings](https://support.apple.com/guide/deployment/airplay-security-payload-settings-dep09c789dce/web), the access modes a school sets on its Apple TVs
- [Apple Developer: AirPlaySecurity](https://developer.apple.com/documentation/devicemanagement/airplaysecurity) and [AirPlay payload](https://developer.apple.com/documentation/devicemanagement/airplay)
- [Unofficial protocol specification](https://git.sudo.is/mirrors/openairplay-airplay-spec/src/branch/master/index.html): `/info`, TXT record fields (`pw`), mirroring
- [pyatv](https://github.com/postlund/pyatv), Python client for Apple TV, with pairing ([protocol notes](https://pyatv.dev/documentation/protocols/))
- [UxPlay](https://github.com/FDH2/UxPlay), open source mirroring receiver for Linux: a test receiver, and a reference for pairing and the password
- [shairport-sync](https://github.com/mikebrady/shairport-sync), open source audio receiver, AirPlay 2 pairing
- [openairplay2](https://github.com/st3fan/openairplay2), open source receiver; its release notes say AirPlay 2 replaces the old password with pairing
