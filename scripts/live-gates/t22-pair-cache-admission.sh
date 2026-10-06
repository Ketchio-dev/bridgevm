#!/usr/bin/env bash
# These flat directories are the D10 repository Python import roots.
set -euo pipefail
/usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin /usr/bin/perl -MFcntl=:mode -e '
    use strict; use warnings; alarm 30;
    my $root = shift @ARGV; my $count = 0;
    for my $directory ("$root/scripts", "$root/scripts/live-gates") {
        my @before = lstat($directory);
        @before && S_ISDIR($before[2]) or die "D10 import root unavailable\n";
        opendir(my $handle, $directory) or die "D10 import inventory unavailable\n";
        my @opened = stat($handle);
        @opened && $opened[0] == $before[0] && $opened[1] == $before[1]
            or die "D10 import root changed\n";
        while (1) {
            $! = 0; my $name = readdir($handle);
            if (!defined $name) { $! == 0 or die "D10 import inventory failed\n"; last; }
            next if $name eq "." || $name eq "..";
            ++$count <= 4096 or die "D10 import inventory exceeds bound\n";
            lc($name) ne "__pycache__" && $name !~ /\.(?:pyc|pyo)\z/i
                or die "D10 cached repository code refused\n";
        }
        my @after = lstat($directory);
        @after && S_ISDIR($after[2]) && $after[0] == $before[0] && $after[1] == $before[1]
            && $after[9] == $before[9] && $after[10] == $before[10]
            or die "D10 import root changed during inventory\n";
        closedir($handle) or die "D10 import inventory close failed\n";
    }
' -- "${1:?source root}"
