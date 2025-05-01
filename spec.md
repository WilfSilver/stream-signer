# Cryptographic Streamed Video Signing and Verification

Issue tracker: [github.com/WilfSilver/stream-signer/issues](https://github.com/WilfSilver/stream-signer/issues)

## Table of Contents

- [Cryptographic Streamed Video Signing and Verification](#cryptographic-streamed-video-signing-and-verification)
   * [Table of Contents](#table-of-contents)
   * [Introduction](#introduction)
      + [Background](#background)
      + [General description](#general-description)
      + [Out of scope](#out-of-scope)
      + [Future goals](#future-goals)
      + [Assumptions](#assumptions)
      + [Existing Solutions](#existing-solutions)
   * [Terminology](#terminology)
   * [Core Data Model](#core-data-model)
      + [Overview](#overview)
      + [Sign file](#sign-file)
      + [Chunk Signature](#chunk-signature)
   * [Basic Concepts](#basic-concepts)
      + [Chunks](#chunks)
         - [Time range](#time-range)
         - [Position](#position)
         - [Size](#size)
         - [Channels](#channels)
         - [Limits](#limits)
      + [Verifiable Presentation](#verifiable-presentation)
   * [Advanced Concepts](#advanced-concepts)
      + [Time to Frame Conversion](#time-to-frame-conversion)
      + [Linking Audio to Frame](#linking-audio-to-frame)
      + [Signing Chunks](#signing-chunks)
      + [Embedding Chunks](#embedding-chunks)
      + [Obtaining Public Keys](#obtaining-public-keys)
   * [Considerations](#considerations)
      + [Privacy](#privacy)
      + [Security](#security)
      + [Accessibility](#accessibility)
      + [Operational](#operational)
   * [User Interface Recommendations](#user-interface-recommendations)
   * [Benchmarking](#benchmarking)
   * [Limitations](#limitations)
      + [Embedding](#embedding)
      + [Performance](#performance)
   * [Integration into Existing Infrastructure](#integration-into-existing-infrastructure)
   * [Acknowledgements](#acknowledgements)
   * [References](#references)

## Introduction

### Background

The rise of disinformation, AI-generated content and deepfakes recently
has caused recent panic around how to detect this and prevent the spread of
information which could harm a company or individual.

Recent examples of these include cryptocurrency scams deep faking important and
respected figures to gain authenticity in the coin to drive investment. This
both affects the end Verifiers trust of the individual, sowing distrust in both the
content which they see online as well as the individual or business themselves.

### General description

Currently, verifying the authenticity of a video -- especially when embedded
inside another -- provides a significant challenge and a lot of manual work.

We propose a solution to this, using cryptographic signatures for signing
streamed video in chunks and specific resolutions. This allows a third party to
verify the origins and authorisation of the created video. Furthermore, a
"chunk" can independently be embedded into another video while keeping its
signature valid while also preventing the abuse of signing frames allowing for
splicing.

A list of stakeholders is as follows:

- Verifiers watching a specific video
- Journalists or other content creators embedding quotations as evidence for
  their own videos
- Content creators, businesses, governments or any individual whom could be the
  target of disinformation or deepfaked content
- Video streaming platforms having to store videos and process the signatures
  presenting them to the verifier for authentication, changing the resolution on-demand
- Browsers and other software requiring to validate and provide a
  understandable UI to the verifier.
- Video editing software which support the embedding of signed video
- Video recording and streaming software to allow the signing process to
  happen while recording a specific video.
- Video call applications where one would want to verify the authenticity of
  the person they are talking to

### Out of scope

- Specific GUI design to combat videos or websites mimicking design aspects to
  appear as if signed.

### Future goals

- Have two different signatures, one for context and one for general integrity,
  allowing context to be removed while keeping the signatures, but making this
  action detectable and thus allowing a warning to be shown to the user that
  context is missing.
- Creating a custom video format to make embeddings as well as reduction of
  resolution easier for streaming platforms to manage and keep signatures valid
  while providing videos.

### Assumptions

There are not many conditions for the user to run the software, due to the mass
use of signing and verification in everyday life on the internet.

- Anyone who is signing a video has already set up a signing key and
  understand how to securely store and manage it

### Existing Solutions

There exist many different approaches currently being explored to make it more
difficult, these focus on detecting AI-generated content, with the two
main ideas being:

1. JPEG Trust - This works by logging all changes made to an image and what
   device took the image etc. in tamperproof metadata. This makes it so that
   you can verify an image was taken by a camera and can verify how it has been
   manipulated to make sure the image is not misleading.

   This, however, does not work on videos, nor is planned to. When trying to
   use a similar method for videos it becomes apparent at how difficult it
   would be to format the modifications in such a way that is understandable
   and easy to detect abuse (due to overlays and streaming services changing
   video resolution on the fly).

2. Watermarking - This is currently the system that is being implemented at the
   moment on a per generative AI model basis. The companies then release often
   tools for detecting these watermarks. These have been applied to video
   however, there is a general consensus of how strong they are, with many
   examples of people writing programs to automatically remove these watermarks
   for you, significantly lowering the barrier to entry.

   Furthermore, due to the information about how these models work, one can
   always create their own (or use an open-source model, writing it to not
   generate watermarks) and run it locally however expensive this may be.

For solutions which focus on signing streamed video, there have been multiple
papers written, all proposing roughly the same system. This system includes
signing each individual frame and its respective audio information and adding
this as either metadata or embedding it in unused pixel data on the frame. This
allows splicing clips together and cutting off context of a point -- which has
proven multiple times in the past can ruin someone's reputation and
credibility. These also don't provide a dedicated implementation or
specification for this process, only given broad high-level overviews of how it
should work resulting in a complete lack of support or actual use.

## Terminology

- Chunk -- A collection of cropped frames and specific audio channels which are signed as one entity
- Deepfake -- Content that has been manipulated such that one object seems or appears as another
- Disinformation -- Misrepresentative information used to deceive the consumer
- Embedding -- Part of a video being integrated into another
- Quote -- An embedding which has an associated signature
- Signer -- An organisation or individual which signs a given video
- SSRT file -- A file of the SRT format specific for storing the chunk signatures for a given video
- Streamed video -- A video that sections are sent to a client such that not all the video is stored on the client's machine
- Verifier -- An organisation or individual which verifies a given video

## Core Data Model

### Overview

The proposed solution of using cryptographic signing, focuses not on detecting
AI-generated content or deep faked content (due to the assumption that this
technology will always be able to be written with watermarking removed),
and instead on the authorisation of a video's existence by a company or
individual.

This system allows embedding quotations in other videos, without the express
permission from the original content creator while remaining signed. Benefiting
journalistic practices of quotations and other such forms.

However, due to how the signature is generated, no overlay can be present on
these quotes and they are limited to specific resolutions which have been
signed by the original party. This then also can cause issues for streaming
platforms, who usually compress videos to reduce storage cost and also change
the resolution depending on the verifiers' internet connection or preference.
Therefore, it is then up to the platforms to validate that the given content is
already compressed to their satisfaction and signed for all potential
resolutions for streaming.

This also brings up a further issue, which is that if you are signing for
multiple resolutions, the compression algorithm you used must be known to the
streaming platform and deterministic -- although most are.

### Sign file

The core of this specification builds upon subtitle (`.srt`) files, which are
common place within video formats, allowing for easy integration into existing
solutions. This format contains a time range that some text applies to, usually
used for showing subtitles at a specific time.

This specification builds upon this to instead inform the verifying application
what data should be included when signing as well as the signature associated
with them, also known as a Chunk Signature, as mentioned below.

Furthermore, via the use of overlapping ranges, it is assumed that the verifier
and signing applications will support having multiple signed chunks at the same
time.

From now on, when referring to the SRT files in the context that they are for
signing, they will be referred to as SSRT files.

### Chunk Signature

A chunk signature needs to define the properties of the chunk that the attached
signature applies for. Given the use of the SSRT file, the time range in which
the signature applies can be assumed. Additionally, the credentials of who
signed the video need to be included.

These values can then be stored within a JSON **\[JSON\]** format and saved to the SSRT file,
so for example:

```json
{
  "pos": {
    // Vector object determining where the position starts
    "x": 0, // Unsigned integeter between 0 and the width of the video
    "y": 0 // Unsigned integeter between 0 and the height of the video
  },
  "size": {
    // Vector object determining the size of the area which has been signed
    "x": 1920, // Unsigned integeter between 0 and the width - pos.x of the video
    "y": 1080 // Unsigned integeter between 0 and the height - pos.y of the video
  },
  "channels": [
      // Array of audio channels that should be included when signing the video
      0,
  ],
  "credential": {
    // If the credential already has been defined, we can simple refer to its ID number
    // defined in its definition

    "id": "http://example.edu/credentials/1872", // The ID specified when defining a credential
  } | {
    "id": "http://example.edu/credentials/1872", // The ID specified when defining a credential
    "jwt": "eyJraWQiOiJkaWQ6dGVzdDpVTnJYdmZjN3dJdnJPazVnbVl4a2U4YWhIYUxKSmp5byNNVW9KVHFwOHRvY1J1UTRDQTRRbE9MdC15anFES1RscFVoaUY4MXpXbi1zIiwidHlwIjoiSldUIiwiYWxnIjoiRWREU0EifQ.eyJpc3MiOiJkaWQ6dGVzdDpVTnJYdmZjN3dJdnJPazVnbVl4a2U4YWhIYUxKSmp5byIsIm5iZiI6MTc0NTU4OTc0OCwidnAiOnsiQGNvbnRleHQiOiJodHRwczovL3d3dy53My5vcmcvMjAxOC9jcmVkZW50aWFscy92MSIsInR5cGUiOiJWZXJpZmlhYmxlUHJlc2VudGF0aW9uIiwidmVyaWZpYWJsZUNyZWRlbnRpYWwiOlsiZXlKcmFXUWlPaUprYVdRNmRHVnpkRHBIVm1rM2VHTlBWbFZHWlRoRVFsSklaamRKTUdKQ1ZqVmtkSGs1YlRCSk5DTkhkMGhEUlVoTloxSmllR3RUZDNKbFVFeFdTems0WW10S2QxcEpSbTB6WkVwM1IwNWlOek51Tm1Kdklpd2lkSGx3SWpvaVNsZFVJaXdpWVd4bklqb2lSV1JFVTBFaWZRLmV5SnBjM01pT2lKa2FXUTZkR1Z6ZERwSFZtazNlR05QVmxWR1pUaEVRbEpJWmpkSk1HSkNWalZrZEhrNWJUQkpOQ0lzSW01aVppSTZNVGMwTlRVNE9UYzBOeXdpYW5ScElqb2lhSFIwY0hNNkx5OXNiMk5oYkdodmMzUXZZM0psWkdWdWRHbGhiSE12U0ZKblRrZ3hiMnhrVTFwRFVrdHZVbkZrTVcxNFIxbDJNM1JtU0U1QmRHa2lMQ0p6ZFdJaU9pSmthV1E2ZEdWemREcFZUbkpZZG1aak4zZEpkbkpQYXpWbmJWbDRhMlU0WVdoSVlVeEtTbXA1YnlJc0luWmpJanA3SWtCamIyNTBaWGgwSWpvaWFIUjBjSE02THk5M2QzY3Vkek11YjNKbkx6SXdNVGd2WTNKbFpHVnVkR2xoYkhNdmRqRWlMQ0owZVhCbElqcGJJbFpsY21sbWFXRmliR1ZEY21Wa1pXNTBhV0ZzSWl3aVZXNXBkbVZ5YzJsMGVVUmxaM0psWlVOeVpXUmxiblJwWVd3aVhTd2lZM0psWkdWdWRHbGhiRk4xWW1wbFkzUWlPbnNpUjFCQklqb2lOQzR3SWl3aVpHVm5jbVZsSWpwN0luUjVjR1VpT2lKQ1lXTm9aV3h2Y2tSbFozSmxaU0lzSW01aGJXVWlPaUpDWVdOb1pXeHZjaUJ2WmlCVFkybGxibU5sSUdGdVpDQkJjblJ6SW4wc0ltNWhiV1VpT2lKQmJHbGpaU0o5ZlgwLmFqRDRHZUJxQi1LMVFMel83MUxkMmhTV3B5QkZIQkg0VDVLeFlGaVVjSk10UXFrZDlaTmRXRjU5TlgtRWR0MDBxSlBJeEdhWGRMdDFubmw2SXVSX0N3Il19fQ.CxiyY12hvqsYtLZrYFKcL9cm1YXY8w2pgmtbFdJ5pK7_1cHt1S6eJRtRPf-MEurZlqoc_T303xmw5Ukh0Kl8AQ", // The encoded JWT containing the Verifiable Presentation
  },
  "signature": "..." // The signature itself in Base64
}
```

For further explanation on the definition of chunks, please see the
[Chunks section](#chunks).

It should be noted that the `credential` key has two different states, either
it is seen as a reference, where only the `id` is specified. This `id` is
file specific and relies on the alternative format of definition to be
specified at an earlier timestamp. This definition includes the full Verifiable Presentation
**\[VERIFIABLE-CREDENTIAL\]** under `pres` as well as defining and ID for that presentation.

This is done to help reduce the file size of a given format, allowing the
presentation to only be included once throughout the SSRT file.

For further description of the presentation, please see the [Verifiable
Presentation section](#verifiable-presentation).

## Basic Concepts

### Chunks

A chunk is a core concept throughout this specification and defines the pixels and audio
to be included within the signature. All chunks are also seen as embeddings, so, as
mentioned below, all chunks must have a position and size defined.

Please see the [Signing Chunks](#signing-chunks) to see a rigorous definition of how
to convert a chunk definition to the data which should be included when signing.

As shown above, the chunks have 4 main properties:

#### Time range

This represents the range of frames which should be taken within the video. As
these are stored in the SRT file, it is floored to the milliseconds, with the
range representing as such: `(start, end]`.

This means that the end timestamp should be the start of the following frame, which is
not included within the timestamp.

#### Position

This is the starting `x` and `y` value for the crop, indexed from 0 in the top left
corner.

If this were a full frame signature, the value would be `{ "x": 0, "y": 0 }`.

If either x or y is greater than the size of the frame minus the given `size`, the
chunk definition should be considered invalid.

#### Size

This defines the width (`x`) and height (`y`) of the crop to perform starting at the
given position.

If this were a full frame signature on 1080p video, the value would be `{ "x": 1920, "y": 1080 }`.

If either x or y is either greater than the size of the video or 0, it should be
considered invalid.

#### Channels

This is the channels, indexed from 0, which should be included in the signature. This
allows embeddings to have separate audio channels to the main video, enabling voice-overs.
It also allows the option to sign without audio giving a third party more flexibility with how
they embed video.

If the signer wanted to include all audio on a stereo channel video, the value would be
`[0, 1]`

If any channel defined does not exist on the video, the chunk definition should be
considered invalid.

#### Limits

To help signing and verification applications predict how much memory is required to
buffer, a limit has been imposed on the length of a chunk. Within this definition,
the minimum duration of a chunk should be `50 milliseconds`. The maximum length
should be `10 seconds` allowing for the signer to choose to require context or not.

This variation in the chunk length is to give the signer choice in what must be grouped
together for a signature to be valid.

If a chunk is either shorter than the minimum or larger than the maximum, the chunk
definition should be considered invalid.

### Verifiable Presentation

This project leverages Verifiable Credentials **\[VERIFIABLE-CREDENTIAL\]** for all
cryptographic capabilities.

This system introduces a decentralised model for securely sharing authenticity and
credibility of a given organisation or subject. In addition to this, a public key
infrastructure, which is then used within this system for acquiring public keys to
verify a signature.

A Verifiable Presentation can be created by a signer with contains a list of
Verifiable Credentials which can then be shown to the verifier for more information
about the signer.

Within the specification, the Presentations are shared via the JWT format

## Advanced Concepts

### Time to Frame Conversion

Due to the SRT files working in milliseconds, it is key to be able to reliably
convert these milliseconds to frame indexes to know what frames will be included
within a given chunk.

Please note that mentions of presentation time here refers to the relative time
of the frame, relative to zero. Meaning that the starting frame should be
normalised to start at 0ms.

When signing, it is assumed the end index and start timestamp is known,
therefore, the start timestamp must be converted into an index and the end
index into a timestamp.

To convert the end index to a timestamp, simply the frame duration is added to
the frame's presentation time and milliseconds floored. Though it should be
noted that mitigations should be put in place for frame rates > 1000fps due to
the potential that flooring the millisecond will change the frame that would be
used.

To then convert the start timestamp to, the milliseconds should be floored and
then simply the `convert_to_index` function mentioned later can be used.

When verifying, the starting index is simply the first frame that's presentation
time is within the defined chunk boundary: [start, end).

The end of the chunk is defined by the `floor` of the frame after the last frame
(so that it is consistent with [start, end)). Therefore, the end index is the
index of the last chunk can be calculated via:

```text
convert_to_index: timestamp * 1 / fps
```

Where `fps` is the frames per second within a float representation.

The result of this when passed, the end of the chunk can then be passed through
`ceil` (as the counterpart of `floor`), to get the index where the chunk should
end.

### Linking Audio to Frame

One key issue is that audio is not assigned to frames, but to make it easier for
knowing what bytes of audio is used within a time range, it is linked to the frame
which is then decided to be included or not by the [time to frame conversion
mentioned above](#time-to-frame-conversion).

Firstly, both the frames and audio is normalised to start at 0ms as mentioned
above. The idea is that the last sample of audio is the last audio which is
solely encompassed by the frame.

the index of the sample can be generated by multiplying the frame end timestamp
(as seconds in float) by the audio rate, which is then floored. This gives the
index of the sample, ignoring both channels and channel width. The start index
is assumed to be the end index of the previous slice, or 0.

By using this method, a slice can be generated over the audio within the video
which is associated with the frame itself.

### Signing Chunks

Once the audio has been linked to the frame and the indexes of the frames to be
included, the data used to sign the chunk, is as follows. The general concept
is producing an array of bytes in the following form for each frame:

```text
[...cropped_frame, ...first_channel, ...second_channel, ...]
```

To generate the bytes required for the cropped frame, the format is assumed to
be reduced to RGB values with 3 bytes per pixel (one for red, one for green and
one for blue). This is then cropped by the defined `pos` and `size` within the
respective chunk (indexed from 0). It is then converted into its flat format,
so each row of bytes is after each other. The resultant vector produced should
be `3 * size.x * size.y`.

Once this has been generated, then the audio for the frame is split into its
channels and the channels included within the chunk definition is then appended
onto the end of the cropped frame in the order in which they appear in the
chunk definition, no deduplication should take place of the channels.

### Embedding Chunks

This specification theoretically allows extraction of frames and related audio
for a given frame, and "embedding" it into another video at any respective point.

However, in practice this can be quite challenging due to the requirement that
the resultant pixel data must stay the same. Therefore, currently the only way
to do this is to sign the original content with losslessly compressed video
and have the video with the embedding also stored with lossless compression.

The frames and audio required to be moved can be defined by the sections above,
and then the chunk definition will need to be updated to account for the offset
or the starting and end timestamps.

In the future, having a custom format that allows multiple video feeds, each
with offsets and scaling specified would enable this specification to be less
reliant on lossless video compression.

### Obtaining Public Keys

Due to the use of Verifiable Credentials **\[VERIFIABLE-CREDENTIAL\]** for the
public key infrastructure. The public key used to verify a signature is the on
that is related to the one used to sign the holder of the presentation. So,
this should be quarriable by any verifier.

## Considerations

### Privacy

No additional personal data is stored on top of the Verifiable Credentials
**\[VERIFIABLE-CREDENTIAL\]** and so there are not many considerations above
this.

Although it should be emphasised that all personal data stored within a shared
credential is viewable to all verifiers.

Additionally, it should also be known that there is a potential for a signer to be
tracked online by their signatures if they use the same public/private key pair for
two different accounts.

### Security

Due to this project detailing how to validate authorisation of an individual,
security is at the heart of considerations. Therefore, all aspects of the
designed specification has been thoroughly reviewed about how it could
potentially be abused to manipulate end users into a false sense of trust.

For example, chunks can be expandable, this allows context to be included in
a signature requiring embeddings to have all information if they wish to be
verifiable. Chunks do have a limit to the length they can be, to make balance
the essence of streaming, but this still can be used to the signers advantage,
by ending the chunk mid-sentence or mid-word, which would make it obvious to the
end user that there was more information to gleam. But this has the
disadvantage that it requires a significant amount of input by the signer,
which could cause complications during a livestream. Workarounds do exist, for
example having software that ends a chunk when the signer or trusted third
party presses a button and starts a new one.

Furthermore, a minimum chunk size is specified to help reduce the chance of
splicing, however full mitigations will be left for the signer to implement,
for example, ending a chunk mid-word. Although, to balance ease of use, it is
also recommended that a simple set chunk size could be used to prevent most
circumstances.

Additionally, the specification has been laid out in such a way that it only
defines what has been signed, with the signature. This means that if a
malicious actor were to try and change any of the aspects of the chunk signature,
the signature itself would be invalidated.

### Accessibility

This project will be open source to provide the source code of the
implementation to anyone who wishes to view it, it is also licensed under
GPL-3 to allow the extension of the system by another party.

Accessibility of the features themselves is determined by the applications
which use this implementation and thus are not considered for this project.

### Operational

Considerations have been made to make this solution as easy to integrate into
existing solutions through the use of subtitles as well as Verifiable
Credentials.

## User Interface Recommendations

This specification only focuses on the signature and verification process and
does not have requirements for how these are shown to the user. However it
should be noted there are potential risks:

1. The video (in full screen) has full control over what the users see, although
   cannot react to users interactions.
2. If a video is embedded within a website, the website has complete control of
   what the users see within the browsers limitations, which includes creating
   dynamic elements which can appear authentic.

Therefore, the following recommendations are given for applications which
support signed video:

- Make the process interactive, for example show up along with the play pause
  button (though this does not tackle the second issue) -- nothing should be
  shown without the users express interaction due to the risk of potential
  forgery.
- Make it expressively obvious that a video has not been signed or has been
  signed, making sure that any forgery is blocked out and made obvious to the
  user.
- As a mitigation for non-interactive elements (or in the browser), a platform
  could opt for having unique and customisable animations for each user. However,
  it should be noted that if users have multiple devices, they may become blind
  to the differences and thus potential to trust unsigned video. Although the
  application itself could try to scan and detect any forgeries mimicking any
  known animation.

## Benchmarking

Benchmarking was done on two systems, one virtualised with one core, to judge
how efficient the example implementation was. Three videos were chosen and
signed at different intervals, with a decoding baseline where no signing
or verifying took place.

This is here to act as an example of the required overhead, though it should
be noted that many optimisations can be made depending on the context in
which is run.

The following table was taken from my dissertation:

![A table showing the overview of results generated by 3 different
videos](./img/benchmark_table.png)

## Limitations

### Embedding

As noted previously, with current video formats, it is required that any
embedding must both be signed with a lossless compression and any subsequent
video which embeds it also stored as lossless compression.

This can be solved with a custom video format that specifically allows
multiple videos to be stored with separate compressions, making it so that
any manipulations do not affect the end pixel values.

### Performance

As shown within the benchmarking, there can be quite a lot of overhead for
verifying on lower end devices for higher resolution video. Although it
should be mentioned that all the results prove that it can be done in real
time. But in conjunction with the current maximum chunk length requiring
an unreasonable amount of memory to be used it could be seen as a burden to
verify all videos.

Improvements to this could be made, especially with the custom format
mentioned above. Though currently this could be seen as a severe limitation
of this solution if all clients were to always verify videos in real-time.

Mitigations could also be made by trusted video streaming platforms, where
their centralised servers verify the videos, storing the results which can
then be sent to the client and shown. Although it should be noted this
practice could lead to malicious websites abusing this trust and show
incorrect signatures to have a video appear more legitimate.

## Integration into Existing Infrastructure

The use of SRT files means that this method can be integrated into existing
infrastructure for streaming videos with subtitles. Within this
specification, it is not mentioned how to determine what subtitle tack stores
the signatures and so this is left up to the project implementing this
specification.

However, currently, for embedding to work, lossless video formats are required
to be used, and therefore it is infeasible in the current streaming landscape.
Although, in the future, this could be bypassed by creating a new container
format which allows multiple streams of video to be contained within a single
format, allowing for the original compressed video to be stored, meaning no
lossless image is required.

## Acknowledgements

## References

**\[JSON\]** -- The JavaScript Object Notation (JSON) Data Interchange Format. T. Bray, Ed.. IETF. December 2017. Internet Standard. URL: https://www.rfc-editor.org/rfc/rfc8259

**\[RFC7519\]** -- JSON Web Token (JWT). M. Jones, J. Bradley, N. Sakimura. May 2015. Internet Standard: https://www.rfc-editor.org/rfc/rfc8259

**\[VERIFIABLE-CREDENTIAL\]** -- Verifiable Credentials Data Model v2.0. M. Sporny, D. Longley, D. Chadwick, I. Herman. March 2025. URL: https://www.w3.org/TR/vc-data-model-2.0/

