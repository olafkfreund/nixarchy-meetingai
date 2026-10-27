# The Quickshell plugin folder: shell/ minus its headless tests.
{ lib, runCommandLocal }:

let
  src = lib.fileset.toSource {
    root = ../shell;
    fileset = lib.fileset.difference ../shell ../shell/tests;
  };
in
runCommandLocal "nixarchy-meetingai-plugin" { } "cp -r ${src} $out"
