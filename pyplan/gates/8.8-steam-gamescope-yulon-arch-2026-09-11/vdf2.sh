#!/bin/bash
V=~/.local/share/Steam/userdata/18347166/config/shortcuts.vdf
echo "# shortcuts.vdf across the whole of T29"
echo "stamp: $(date -Is)"
echo
echo "sha256, four readings:"
echo "  22:03:24  as found                                    e4068f33648ee2ce87e7f6b74e659ec1ac52d8ed79833ec000cef8a6b7c0e8f0"
echo "  22:43:25  after Half 1 launched the client entry      244a23edd0d2e9236a7a55f2c2f51c669fbba96953841f953cf1e98c6124cb6c"
echo "  23:09:55  after Half 2 run 1 launched the server entry 351bae71561078da484d1c0ee1cd0d0188dabd5fdefe3b2d647fc717f3fc58b3"
printf "  %s  as left, after Half 2 run 2                 " "$(date +%H:%M:%S)"
sha256sum "$V" | cut -d' ' -f1
echo
echo "The sum moved with every launch and the ENTRIES never did. Steam writes LastPlayTime"
echo "into this file whenever a shortcut is launched, and this ticket asked for both entries"
echo "to be launched. That the sum moves is a DEVIATION from the ticket's 'shortcuts.vdf"
echo "untouched (sha256 before and after)' and is listed as one in both READMEs."
echo
echo "HONESTY NOTE: the as-found file was only ever HASHED, never parsed -- no copy of it was"
echo "kept, so the claim that the entries are unchanged rests on comparing the parses below"
echo "with what T17's live run recorded and with the 8.8 gate's own listing, plus the fact"
echo "that the file is the same 681 bytes at every reading. It is not a parse-to-parse diff"
echo "against the as-found bytes."
echo
echo "Below: the file as left, read with the app's own codec (yulon.steam.vdf_parse)."
