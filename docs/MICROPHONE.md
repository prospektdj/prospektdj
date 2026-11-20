# Microphone

In Settings → Audio → Mikrofon, enable capture and choose the default system input or refresh/select an input device. The MIC knob beside MASTER/PHONES controls its level; zero mutes it. Capture is off by default. The input selection, level and enabled preference are saved.

The system capture backend (WASAPI on Windows, system/ALSA on Linux) supplies input channel 1 as mono to both MASTER channels, before the MASTER gain, limiter and recording. Microphone audio is included in recordings and excluded from PFL. Use headphones to avoid acoustic feedback. The REC control is a square red-labelled button; it becomes red-filled while recording and stops the recording when clicked again.

A bounded 50 ms queue hands capture frames to the engine without waiting. A streaming linear converter handles different input/output sample rates; overflow discards oldest input rather than accumulating an increasing delay. This is basic software monitoring, not a low latency or independently clocked ASIO input implementation. ASIO output can be used alongside an available system capture endpoint; ASIO-only inputs require an OS capture endpoint to appear in this list. Device/open/capture errors disable the microphone and report a message without stopping MASTER playback. Audio-output restart reopens the input for the new engine/sample rate.

Automated tests cover gain, mute, PFL isolation, disconnect, sample-rate conversion and microphone inclusion in recorded MASTER blocks. Physical microphone/driver latency and acoustic behaviour require hardware testing.
