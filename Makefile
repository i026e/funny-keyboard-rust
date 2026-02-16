# KDIR ?= /lib/modules/`uname -r`/build
KDIR ?= ./kernel/linux618
MODULE_NAME := funny_kbd

obj-m := $(MODULE_NAME).o


all:
	make LLVM=1 -C $(KDIR) M=$(PWD) modules

clean:
	make LLVM=1 -C $(KDIR) M=$(PWD) clean

prep:
	make LLVM=1 -C $(KDIR) rustprep

