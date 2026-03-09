package main

import "net/http"

type UserService struct {
    repo *UserRepository
}

func (s *UserService) GetUser(w http.ResponseWriter, r *http.Request) {
    user := s.repo.Find(r.URL.Query().Get("id"))
    json.NewEncoder(w).Encode(user)
}

func HandleHealth(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("ok"))
}

type UserRepository interface {
    Find(id string) *User
    Save(user *User) error
}
