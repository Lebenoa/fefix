#!/usr/bin/env fish
# Fish completion script for Helix editor

complete -c fx -s h -l help -d "Prints help information"
complete -c fx -l strict -d "Bail on error for commands that can fail"
complete -c fx -l tutor -d "Loads the tutorial"
complete -c fx -l health -xa "(__fx_langs_ops)" -d "Checks for errors"
complete -c fx -l health -xka all -d "Prints all diagnostic informations"
complete -c fx -l health -xka all-languages -d "Lists all languages"
complete -c fx -l health -xka languages -d "Lists user configured languages"
complete -c fx -l health -xka clipboard -d "Prints system clipboard provider"
complete -c fx -s g -l grammar -x -a "fetch build" -d "Fetch or build tree-sitter grammars"
complete -c fx -s v -o vv -o vvv -d "Increases logging verbosity"
complete -c fx -s V -l version -d "Prints version information"
complete -c fx -l vsplit -d "Splits all given files vertically"
complete -c fx -l hsplit -d "Splits all given files horizontally"
complete -c fx -s c -l config -r -d "Specifies a file to use for config"
complete -c fx -l log -r -d "Specifies a file to use for logging"
complete -c fx -s w -l working-dir -d "Specify initial working directory" -xa "(__fish_complete_directories)"

function __fx_langs_ops
    fx --health all-languages | tail -n '+2' | string replace -fr '^(\S+) .*' '$1'
end
