---
title: Assembly Programming Basics
date: 2026-10-11
tags: [asm, assembly]
summary: The handful of commands I actually use every week.
---

## Section 1 - Computer Engineering Fundamentals

### How processors work?

[![assembly 101](/assets/images/assembly-101.png)](/assets/images/assembly-101.png)

The chip on left is ROM which stores the data ($D_0 - D_n$) in the address($A_0-A_n$) locations. Both data and address location pins are connected together, this will raise a question of collision this is where clock comes in play. Clock as we know in processor, GHz, MHz, they run through a cycle, during one cycle the Data will be sent, and during another cycle address location will be read. This will raise another question of when to read/write this is where R/W signal comes into play. 

In order to work on a processor we need something called a Instruction set, which is nothing but instruction code and the description of it, instruction code is also known as an Opcode. 

IP or instruction pointer points to the entry point of the processor where the operation will start and move sequentially. 

## Intro to 8086

[embed](/assets/pdfs/8086.pdf)