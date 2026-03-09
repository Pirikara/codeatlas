package main

import (
    "encoding/json"
    "net/http"
)

type Handler struct {
    service *UserService
}

func (h *Handler) CreateUser(w http.ResponseWriter, r *http.Request) {
    name := r.FormValue("name")
    password := r.FormValue("password")
    h.service.CreateUser(name, password)
    w.WriteHeader(http.StatusCreated)
}

func (h *Handler) GetUser(w http.ResponseWriter, r *http.Request) {
    id := r.URL.Query().Get("id")
    user := h.service.FindUser(id)
    json.NewEncoder(w).Encode(user)
}

func NewHandler(svc *UserService) *Handler {
    return &Handler{service: svc}
}
