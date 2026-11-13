# Default colors for interactive workspace Terminals.
case $- in
    *i*) ;;
    *) return ;;
esac

# G5 supplies TERM=xterm-256color. Keep plain terminals free of color escapes.
case ${TERM:-dumb} in
    xterm*|screen*|tmux*|*-256color)
        PS1='\[\e[32m\]\u@\h\[\e[0m\]:\[\e[34m\]\w\[\e[0m\]\$ '
        alias ls='ls --color=auto'
        alias grep='grep --color=auto'
        ;;
esac
