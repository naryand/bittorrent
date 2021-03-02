#include <stdio.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/types.h>
#include "bencode.h"
#define MAX_SIZE 65536

int main(void) {
    int fp;
    char fileName[] = "./a.torrent";
    char *str;

    fp = open(fileName, O_RDONLY);
    if(!fp) {
        printf("Failed to load\n");
        exit(1);
    }
    str = (char *) calloc(MAX_SIZE, 1);
    read(fp, str, MAX_SIZE);
    close(fp);

    List *tree = parse(str);;

    print_tree(tree);
    free_ll(tree);
    free(str);
}