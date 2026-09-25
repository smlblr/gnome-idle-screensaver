#!/usr/bin/env bash
# cpu.sh PID SECONDS -> CPU % (100 = one core) over the interval and current RSS
pid=$1; secs=$2; tck=$(getconf CLK_TCK)
ticks() { awk '{print $14+$15}' /proc/$1/stat; }
a=$(ticks $pid); t0=$(date +%s.%N); sleep "$secs"; b=$(ticks $pid); t1=$(date +%s.%N)
awk -v a=$a -v b=$b -v t=$tck -v t0=$t0 -v t1=$t1 -v p=$pid 'BEGIN{printf "pid %s cpu=%.2f%% over %.1fs", p, (b-a)/t/(t1-t0)*100, t1-t0}'
echo " rss=$(awk '/VmRSS/{print $2" "$3}' /proc/$pid/status)"
