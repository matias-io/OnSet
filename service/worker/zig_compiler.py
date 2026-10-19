"""User-local C compiler bridge for CPython's GNU-specific symbol-hiding flag."""
import os,sys

if __name__=='__main__':
    # This GNU ld visibility flag is unsupported by zig cc; extension entry points
    # remain explicit PyInit_* exports and the generated numerical code is unchanged.
    arguments=[arg for arg in sys.argv[2:] if arg!='-Wl,--exclude-libs,ALL']
    os.execv(sys.executable,[sys.executable,'-m','ziglang',sys.argv[1],*arguments])
