#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    void *data;
    struct Node *next;
} Node;

typedef struct {
    Node *head;
    Node *tail;
} List;

void append(List *list, void *data) {
    Node *node = malloc(sizeof(Node));
    node->data = data;
    node->next = NULL;
    if(list->head == NULL) {
        list->head = node;
        list->tail = node;
    } else {
        list->tail->next = node;
        list->tail = node;
    }
}

typedef struct {
    char *key;
    void *value;
} Pair;

char *str = "li12e4:teste";

int *parse_int(char *str) {
    int *num = malloc(sizeof(int));
    sscanf(str, "i%de", num);
    return num;
}

char *parse_str(char *str) {
    int len;
    sscanf(str, "%d:", &len);
    char *ret = malloc(len);
    char format[10];
    snprintf(format, 10, "%%d:%%%ds", len);
    sscanf(str, format, &len, ret);
    return ret;
}

// uses parser advancing logic
int get_list_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') {
        if(*str == 'i') {
            for(; *str != 'e'; str++) {} str++;
        } else if(*str == 'l') {
            str += get_list_len(str);
        } else {
            int len;
            sscanf(str, "%d:", &len);
            str += len+2;
        }
    }
    return (str-start)+1;
}

List *parse_list(char *str) {
    List *list = malloc(sizeof(List)); str++;
    while(*str != 'e') {
        if(*str == 'i') {
            append(list, parse_int(str));
            // advance past int
            for(; *str != 'e'; str++) {} str++;
        } else if(*str == 'l') {
            append(list, parse_list(str));
            // advance past list
            str += get_list_len(str);
        } else {
            append(list, parse_str(str));
            // advance past str
            int len;
            sscanf(str, "%d:", &len);
            str += len+2;
        }
    }
    return list;
}

int main(void) {
    List *list = parse_list(str);
    printf("%d\n", *(int *)list->head->data);
    printf("%s\n", (char *)list->tail->data);
    printf("%d\n", get_list_len(str));
}
