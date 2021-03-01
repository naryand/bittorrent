#include <stdio.h>
#include <stdlib.h>

enum type{INT, STRING, PAIR, LIST, DICT};

typedef struct Node {
    void *data;
    struct Node *next;
    char type;
} Node;

typedef struct {
    Node *head;
    Node *tail;
} List;

void append(List *list, void *data, char type) {
    Node *node = malloc(sizeof(Node));
    node->data = data;
    node->next = NULL;
    node->type = type;
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
    char type;
} Pair;

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
    int digits = 15; // max digits of len
    char format[5+digits];
    snprintf(format, 5+digits, "%%d:%%%ds", len);
    sscanf(str, format, &len, ret);
    return ret;
}

int get_str_len(char *str) {
    int len;
    sscanf(str, "%d:", &len);
    int i = 0;
    for(; str[i] != ':'; i++) {}
    return len+i+1;
}

int get_dict_len(char *str);

// uses parser advancing logic
int get_list_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') {
        if(*str == 'i') str += get_int_len(str);
        else if(*str == 'l') str += get_list_len(str);
        else if(*str == 'd') str += get_dict_len(str);
        else str += get_str_len(str);
    }
    return (str-start)+1;
}

List *parse_dict(char *str);

List *parse_list(char *str) {
    List *list = malloc(sizeof(List)); str++;
    while(*str != 'e') {
        if(*str == 'i') {
            append(list, parse_int(str), INT);
            // advance past int
            str += get_int_len(str);
        } else if(*str == 'l') {
            append(list, parse_list(str), LIST);
            // advance past list
            str += get_list_len(str);
        } else if(*str == 'd') {
            append(list, parse_dict(str), DICT);
            // advance past dict
            str += get_dict_len(str);
        } else {
            append(list, parse_str(str), STRING);
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
    str += get_str_len(str);
    if(*str == 'i') {
        pair->val = parse_int(str);
        pair->type = INT;
    } else if(*str == 'l') {
        pair->val = parse_list(str);
        pair->type = LIST;
    } else if(*str == 'd') {
        pair->val = parse_dict(str);
        pair->type = DICT;
    } else {
        pair->val = parse_str(str);
        pair->type = STRING;
    }
    return pair;
}

int get_pair_len(char *str) {
    int len = get_str_len(str);
    str += len;
    if(*str == 'i') len += get_int_len(str);
    else if(*str == 'l') len += get_list_len(str);
    else if(*str == 'd') len += get_dict_len(str);
    else len += get_str_len(str);
    return len;
}

List *parse_dict(char *str) {
    List *dict = malloc(sizeof(List)); str++;
    while(*str != 'e') {
        append(dict, parse_pair(str), PAIR);
        str += get_pair_len(str);
    }
    return dict;
}

int get_dict_len(char *str) {
    char *start = str; str++;
    while(*str != 'e') str += get_pair_len(str);
    return (str-start)+1;
}

List *parse(char *str) {
    List *tree = malloc(sizeof(List));
    while(*str != 0 && *str != 'e') {
        if(*str == 'i') {
            append(tree, parse_int(str), INT);
            // advance past int
            str += get_int_len(str);
        } else if(*str == 'l') {
            append(tree, parse_list(str), LIST);
            // advance past list
            str += get_list_len(str);
        } else if(*str == 'd') {
            append(tree, parse_dict(str), DICT);
            // advance past dict
            str += get_dict_len(str);
        } else {
            append(tree, parse_str(str), STRING);
            // advance past str
            str += get_str_len(str);
        }
    }
    return tree;
}

void print_list(List *list);

void print_dict(List *dict) {
    printf("{");
    for(Node *cur = dict->head; cur != NULL; cur = cur->next) {
        Pair *pair = cur->data;
        char *key = pair->key;
        if(pair->type == INT) {
            int *num = pair->val;
            printf("%s:%d, ", key, *num);
        } else if(pair->type == LIST) {
            printf("%s:", key);
            print_list(pair->val);
            printf(", ");
        } else if(pair->type == DICT) {
            printf("%s:", key);
            print_dict(pair->val);
            printf(", ");
        } else if(pair->type == STRING) {
            char *val = pair->val;
            printf("%s:%s, ", key, val);
        }
    }
    printf("} ");
}

void print_list(List *list) {
    printf("[");
    for(Node *cur = list->head; cur != NULL; cur = cur->next) {
        if(cur->type == INT) {
            int *num = cur->data;
            printf("%d, ", *num);
        } else if(cur->type == LIST) {
            print_list(cur->data);
            printf(", ");
        } else if(cur->type == DICT) {
            print_dict(cur->data);
            printf(", ");
        } else if(cur->type == STRING) {
            char *val = cur->data;
            printf("%s, ", val);
        }
    }
    printf("] ");
}

void print_tree(List *tree) {
    for(Node *cur = tree->head; cur != NULL; cur = cur->next) {
        if(cur->type == INT) {
            int *num = cur->data;
            printf("%d\n", *num);
        } else if(cur->type == LIST) {
            print_list(cur->data);
            printf("\n");
        } else if(cur->type == DICT) {
            print_dict(cur->data);
            printf("\n");
        } else if(cur->type == STRING) {
            char *val = cur->data;
            printf("%s\n", val);
        }
    }
}

void free_ll(List *tree) {
    for(Node *cur = tree->head; cur != NULL; cur = cur->next) {
        if(cur->type == LIST || cur->type == DICT) {
            free_ll(cur->data);
        } else if(cur->type == PAIR) {
            Pair *pair = cur->data;
            free(pair->key);
            free(pair->val);
            free(pair);
        } else { 
            free(cur->data);
        }
    }
    Node *f = tree->head;
    Node *g;
    while(f != NULL) {
        g = f;
        f = f->next;
        free(g);
    } 
    free(tree);
}