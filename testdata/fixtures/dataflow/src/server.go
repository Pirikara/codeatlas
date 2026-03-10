package main

import "fmt"

func serve(addr string) error {
	handler := createHandler(addr)
	fmt.Println(handler)
	return handler.Start()
}

func createHandler(addr string) *Handler {
	h := &Handler{Addr: addr}
	return h
}

type Handler struct {
	Addr string
}

func (h *Handler) Start() error {
	return nil
}
