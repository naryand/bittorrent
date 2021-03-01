#include <stdio.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include "bencode.h"
#define MAX_SOURCE_SIZE 65536

int main(void) {
    int fp;
    char fileName[] = "./a.torrent";
    char *source_str;

    fp = open(fileName, O_RDONLY);
    if(!fp) {
        printf("Failed to load\n");
        exit(1);
    }
    source_str = (char *) malloc(MAX_SOURCE_SIZE);
    read(fp, source_str, MAX_SOURCE_SIZE);
    // source_str = mmap(NULL, MAX_SOURCE_SIZE, PROT_READ, MAP_SHARED, fp, 0);
    close(fp);

    List *tree = parse(source_str);;

    print_tree(tree);
    free_ll(tree);
    // munmap(source_str, MAX_SOURCE_SIZE);
    free(source_str);
}