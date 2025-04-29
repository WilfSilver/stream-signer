# Cryptographic Streamed Video Signing and Verification

Issue tracker: [github.com/WilfSilver/stream-signer/issues](https://github.com/WilfSilver/stream-signer/issues)

## Introduction

### General description

Currently, verifying the authenticity of a video -- especially when embedded
inside another -- provides a significant challenge and a lot of manual work.

We propose a solution to this, using cryptographic signatures for signing
streamed video in chunks and specific resolutions. This allows a third party to
verify the origins and authorisation of the created video. Furthermore, a
"chunk" can independently be embedded into another video while keeping its
signature valid while also preventing the abuse of signing frames allowing for
splicing.

A list of stakeholders are as follows:

- Users watching a specific video
- Journalists or other content creators embedding quotations as evidence for
  their own videos
- Content creators, businesses, governments or any individual whom could be the
  target of disinformation or deepfaked content
- Video streaming platforms having to store videos and process the signatures
  presenting them to the user for authentication, changing the resolution on-demand
- Browsers and other software requiring to validate and provide a
  understandable UI to the user.
- Video editing software which support the embedding of signed video
- Video recording and streaming software to allow the signing process to
  happen while recording a specific video.
- Video call applications where one would want to verify the authenticity of
  the person they are talking to

### Glossary

- Chunk -
- Deepfake -
- Disinformation -
- Embed -
- Frame -
- Livestream -
- Metadata -
- Overlay -
- Quote -
- Resolution -
- Signature -
- Signer -
- Signing -
- Streamed video -
- User -
- User Interface (UI) -
- Verifying -

### Background

The rise of disinformation, AI generated content and deepfakes in recent years
has caused recent panic around how to detect this and prevent the spread of
information which could harm a company or individual.

Recent examples of these include cryptocurrency scams deepfaking important and
respected figures to gain authenticity in the coin to drive investment. This
both affects the end users trust of the individual, sowing distrust in both the
content which they see online as well as the individual or business themselves.

### Product and technical requirements

### Out of scope

- Specific GUI design to combat videos or websites mimicing design aspects to
  appear as if signed.
- Creating a custom video format to make embeddings as well as reduction of
  resolution easier for streaming platforms to manage and keep signatures valid
  while providing videos.

### Future goals

- Have two different signatures, one for context and one for general integrity,
  allowing context to be removed while keeping the signatures, but making this
  action detectable and thus allowing a warning to be shown to the user that
  context is missing.

### Assumptions

There are not many conditions for the user to run the software, due to the mass
use of signing and verification in every day life on the internet.

- Anyone who is signing a video already has setup a signing key and
  understand how to securely store and manage it

## Solutions

### Existing solution

There exist many different approaches currently being explored to make it more
difficult, these focus on detecting AI generated content, with the two
main ideas being:

1. JPEG Trust - This works by logging all changes made to an image and what
   device took the image etc. in tamper proof metadata. This makes it so that
   you can verify an image was taken on a camera and can verify how its been
   manipulated to make sure the image is not misleading.

   This, however, does not work on videos, nor is planned to. When trying to
   use a similar method for videos it becomes apparent at how difficult it
   would be to format the modifications in such a way that is understandable
   and easy to detect abuse (due to overlays and streaming services changing
   video resolution on the fly).

2. Watermarking - This is currently the system that is being implemented at the
   moment on a per generative AI model basis. The companies then release often
   tools for detecting these watermarks. These have been applied to video
   however there is a general consensous of how strong they are, with many
   examples of people writing programs to automatically remove these watermarks
   for you, significantly lowering the barrier to entry.

   Furthermore, due to the information about how these models work, one can
   always create their own (or use an open source model writing it to not
   generate watermarks) and run it locally however expensive this may be.

For solutions which focus on signing streamed video, there have been multiple
papers written, all proposing roughly the same system. This system includes
signing each individual frame and its respective audio information and adding
this as either metadata or embedding it in unused pixel data on the frame. This
allows splicing clips together and cutting off context of a point -- which has
proven multiple times in the past can ruin someone's reputation and
credibility. These also don't provide a dedicated implementation or
specification for this process, only given broad high level overviews of how it
should work resulting in a complete lack of support or actual use.

### Proposed solution

The proposed solution of using cryptographic signing, focuses not on detecting
AI generated content or deep faked content (due to the assumption that this
technology will always be able to be written with watermarking removed),
and instead on the authorisation of a videos existence by a company or
individual.

This system allows embedding quotations in other videos, without the express
permission from the original content creator while remaining signed. Benefiting
journalistic practices of quotations and other such forms.

However, due to how the signature is generated, no overlay can be present on
these quotes and they are limited to specific resolutions which have been
signed by the original party. This then also can cause issues for streaming
platforms, who usually compress videos to reduce storage cost and also change
the resolution depending on the users internet connection or preference.
Therefore, it is then up to the platforms to validate that the given content is
already compressed to their satisfaction and signed for all potential
resolutions for streaming.

This also brings up a further issue, which is that if you are signing for
multiple resolutions, the compression algorithm you used must be known to the
streaming platform and deterministic -- although most are.

#### Signing information

As we are signing in chunks connected to timestamps, the signatures themselves
will be stored inside a subtitles file, one for each resolution it is being
signed at.

Each timeframe of the subtitles file will contain a JSON-LD object which describes
the id of the credentials which generated the signature as well as the pixel
area which has been signed. This object should look as such:

```json
  // An array of Signature objects
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
    "@context": [
      "https://www.w3.org/2018/credentials/v1",
      "https://www.w3.org/2018/credentials/examples/v1"
    ],
    "id": "http://example.edu/credentials/1872",
    "type": ["VerifiableCredential", "AlumniCredential"],
    "issuer": "https://example.edu/issuers/565049",
    "issuanceDate": "2010-01-01T19:23:24Z",
    "credentialSubject": {
      "id": "did:example:ebfeb1f712ebc6f1c276e12ec21",
      "alumniOf": {
        "id": "did:example:c276e12ec21ebfeb1f712ebc6f1",
        "name": [{
          "value": "Example University",
          "lang": "en"
        }, {
          "value": "Exemple d'Université",
          "lang": "fr"
        }]
      }
    },
    "proof": {
      "type": "RsaSignature2018",
      "created": "2017-06-18T21:19:10Z",
      "proofPurpose": "assertionMethod",
      "verificationMethod": "https://example.edu/issuers/565049#key-1",
      "jws": "eyJhbGciOiJSUzI1NiIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..TCYt5X
        sITJX1CxPCT8yAV-TVkIEq_PbChOMqsLfRoPsnsgw5WEuts01mq-pQy7UJiN5mgRxD-WUc
        X16dUEMGlv50aqzpqh4Qktb3rk-BuQy72IFLOqV0G_zS245-kronKb78cPN25DGlcTwLtj
        PAYuNzVBAh4vGHSrQyHUdBBPM"
    }
  },
  "signature": "..." // The signature itself in Base64
}
```

You will notice `credential` has multiple variations, one to reference a
previously defined credential and another to define a new one. It has been
designed this way to allow for livestreams to include a new credential in the
middle of a stream without having to update the video metadata.

Furthermore, due to the array of signature objects, it allows for multiple
individuals sign a single video as well as embeddings to be included without
having to get the original source to resign your video.

It should be noted however that this subtitle file approach does allow for
single frames to be signed, this should be considered invalid and bad practice.
Instead signing applications should have a minimal signature duration of `0.5 seconds`,
with no maximum set but a recommendation for `10 seconds`. This variation is
intentional to let the user decide what is "chunked" together, allowing them to
make sure that their content cannot be spliced and must include context when
being quoted.

#### User interface recommendations

This specification only focuses on the signature and verification process and
does not have requirements for how these are shown to the user. However it
should be noted there are potential risks:

1. The video (in full screen) has full control over what the users see, although
   cannot react to users interactions.
2. If a video is embedded within a website, the website has complete control of
   what the users sees within the browsers limitations, which includes creating
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
  application itself could try to scan and detect any forgeries mimicing any
  known animation.

### Test plan

All libraries will be fully tested to make sure that everything works as
correctly. These tests will be designed during the writing of the libraries
themselves, however will not be implemented until a basic structure has been
decided upon.

Due to the nature of the application, multiple videos will be used for testing,
some of custom creation and others royalty free or sample media.

### Alternate solutions

## Further considerations

### Third-party services and platforms considerations

The use of a subtitles file is designed to make it as easy as possible for
platforms to add support. Furthermore, the libraries themselves for validating
and signing will be WASM compatible allowing for embedding within websites.
Furthermore, the use of a subtitles file makes this specification format-agnostic
although the libraries will be written to only support AV1 to make it easier
for testing.

However, it is noted, that platforms will probably not perform the verification
on device, instead verifying it on their servers and showing that the
verification passed to the users.

### Cost analysis

The extra cost burden will mostly be on individual users having to verify the
hashes generated, as well as the users generating the signatures in the first
place.

Due to the storage process only requires one extra subtitle format, the
increased cost to the distribution platforms will be negligible.

<!-- TODO:

Calculate how large the subtitle files will be to verify the above statement

-->

### Security considerations

Due to this project detailing how to validate authorisation of an individual,
security is at the heart of considerations. Therefore, all aspects of the
designed specification has been thoroughly reviewed about how it could
potentially be abused to manipulate end users into a false sense of trust.

For example, chunks can be expandable, this allows context to be included in
a signature requiring embeddings to have all information if they wish to be
verifiable. Chunks do have a limit to the length they can be, to make balance
the essence of streaming, but this still can be used to the signers advantage,
by ending the chunk mid sentence or mid word which would make it obvious to the
end user that there was more information to gleam. But this has the
disadvantage that it requires a significant amount of input by the signer,
which could cause complications during a livestream. Work arounds do exist, for
example having software that ends a chunk when the signer or trusted third
party presses a button and starts a new one.

Furthermore, a minimum chunk size is specified to help reduce the chance of
splicing, however full mitigations will be left for the signer to implement,
for example ending a chunk mid word. Although, to balance ease of use, it is
also recommended that a simple set chunk size could be used to prevent most
circumstances.

<!-- TODO:

- making fake credential information to appear as if it was official

-->

### Privacy concerns

Due to the lack of data stored and all information shared is approved by the
signer themselves, there is little concern for privacy.

But it should be made known to the signers that any information included in
their credentials will be viewable to all users, which includes their email.

It should also be known that there is a potential for a user been tracked
online by their signatures if they use the same public/private key pair for
two different accounts.

### Regional considerations

Credential information should support all characters within the UTF-8
specification.

### Accessibility considerations

This project will be open source to provide the source code of the
implementation to anyone who wishes to view it, it is also licensed under
GPL-3 to allow the extension of the system by another party.

Accessibility of the features themselves is determined by the applications
which use this implementation and thus are not considered for this project.

### Operational considerations

Due to this being only an example to use as a basis in which to expand, no
operational considerations are required.

### Risks

Due to this being a proposal with the purpose of showing a working
implementation there is no significant risks to the system.

### Support considerations

Sufficient documentation will be provided throughout the libraries to make.

## Success evaluation

### Impact

### Metrics

## Deliberation

### Discussion

### Open questions

- How will embeddings be affected by downscaling?

## End matter

### Related work

### References

### Acknowledgements
