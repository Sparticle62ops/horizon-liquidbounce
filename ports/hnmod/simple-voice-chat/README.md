# simple-voice-chat (Horizon native mod)

Client-side proximity and group voice chat for the Horizon Rust client. Speaks the Simple Voice Chat 2.6.x protocol (compatibility version 20), so it works with Java servers running the original mod and with the voicechat-server native mod in this registry.

Clean-room build: written from the wire protocol, using the voicechat-server native mod reference in the horizon-mods registry. No code or assets from the original Java mod are included. Credit and link for the protocol the original defined: henkelmax, https://modrinth.com/mod/simple-voice-chat

Push-to-talk, Opus voice (vendored libopus, BSD), positional mixing by server distance, groups. MIT licensed, see LICENSE.
