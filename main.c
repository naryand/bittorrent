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
    void *val;
} Pair;

char *str = "d3:key3:vale";

int *parse_int(char *str) {
    int *num = malloc(sizeof(int));
    sscanf(str, "i%de", num);
    return num;
}

int get_int_len(char *str) {
    char *start = str;
    for(; *str != 'e'; str++) {} str++;
    return str-start;
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

int get_str_len(char *str) {
    int len;
    sscanf(str, "%d:", &len);
    return len+2;
}

// uses parser advancing logic
int get_list_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') {
        if(*str == 'i') str += get_int_len(str);
        else if(*str == 'l') str += get_list_len(str);
        else str += get_str_len(str);
    }
    return (str-start)+1;
}

List *parse_list(char *str) {
    List *list = malloc(sizeof(List)); str++;
    while(*str != 'e') {
        if(*str == 'i') {
            append(list, parse_int(str));
            // advance past int
            str += get_int_len(str);
        } else if(*str == 'l') {
            append(list, parse_list(str));
            // advance past list
            str += get_list_len(str);
        } else if(*str == 'd') {
            append(list, parse_dict(str));
            // advance past dict
            str += get_dict_len(str);
        } else {
            append(list, parse_str(str));
            // advance past str
            str += get_str_len(str);
        }
    }
    return list;
}

Pair *parse_pair(char *str) {
    Pair *pair = malloc(sizeof(Pair));
    pair->key = parse_str(str);
    // advance past str
    int len;
    sscanf(str, "%d:", &len);
    str += len+2;
    if(*str == 'i') pair->val = parse_int(str);
    else if(*str == 'l') pair->val = parse_list(str);
    else if(*str == 'd') pair->val = parse_dict(str);
    else pair->val = parse_str(str);
    return pair;
}

int get_pair_len(char *str) {
    int len = get_str_len(str);
    str += len;
    if(*str == 'i') len += get_int_len(str);
    else if(*str == 'l') len += get_list_len(str);
    else len += get_str_len(str);
    return len;
}

List *parse_dict(char *str) {
    List *dict = malloc(sizeof(List)); str++;
    while(*str != 'e') {
        append(dict, parse_pair(str));
        str += get_pair_len(str);
    }
    return dict;
}

int get_dict_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') str += get_pair_len(str);
    return (str-start)+1;
}

int main(void) {
    List *list = parse_dict(str);
    printf("%s\n", ((Pair *)list->head->data)->key);
    printf("%s\n", (char *)((Pair *)list->head->data)->val);
}