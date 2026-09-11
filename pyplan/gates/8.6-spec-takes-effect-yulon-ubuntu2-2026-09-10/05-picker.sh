#!/usr/bin/env bash
# Step 05 -- the panel's spec picker, now that the install has a deployed conf.
#
# The frame is an OFFSCREEN grab (`QT_QPA_PLATFORM=offscreen`), not a photograph
# of the VM's desktop: T5 shot the desktop through the Hyper-V console and this
# run has no console to shoot. The widget, its seam and its job runner are the
# shipped ones -- `PartyPanel(seam, jobs=threaded_job_runner(window))`, which is
# `_build_my_party_group`'s own line -- so what the frame shows is what the tab
# shows. Only the surface it is painted on differs, and this comment is the
# whole of that difference.
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar
CLASS=mage

say "step 05, reading the panel's spec picker now that the conf is deployed, and grabbing the shipped widget offscreen."
{
hdr "T18 live, step 05: the picker"

sect "what the seam offers, class by class -- the picker's own source"
$PRESS specs

sect "and the conf it read them out of"
$PRESS conf

sect "the shipped PartyPanel, offscreen, with the picker open on $CLASS"
QT_QPA_PLATFORM=offscreen $PRESS frame "$MASTER" "$CLASS" "$GATE/05-picker-frame-$CLASS.png"

sect "the same widget on warlock -- T5's class, the one it photographed refusing"
QT_QPA_PLATFORM=offscreen $PRESS frame "$MASTER" warlock "$GATE/05-picker-frame-warlock.png"

sect "the frames"
ls -l "$GATE"/05-picker-frame*.png

sect "done"
} 2>&1 | tee "$OUT/05-picker.log"
say "step 05 done -- the picker's frames are captured."
