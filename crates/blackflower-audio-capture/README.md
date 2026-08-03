# Blackflower audio capture

`blackflower-audio-capture` owns live microphone input and server-side voice
analysis. The CPAL callback only converts samples into a preallocated lock-free
SPSC ring. Mono conversion, resampling, PTT/VAD, Opus, FEC/PLC, spectral
analysis, allocation, and error inspection run outside the callback.

Live voice packets carry raw bounded Opus frames directly in the versioned
network protocol. Recorded media uses lossless FLAC and does not depend on
Ogg or Opus.
