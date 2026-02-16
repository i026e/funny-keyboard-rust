#!/usr/bin/env sh


make
sudo insmode funny_kbd.ko
sudo dmesg
sudo rmmod funny_kbd