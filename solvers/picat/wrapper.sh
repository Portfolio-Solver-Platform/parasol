#!/bin/bash
# Enter the directory so Picat can find dependent files (parser/tokenizer)
cd /opt/fzn_picat
/usr/local/bin/picat fzn_picat_sat.pi "$@"
