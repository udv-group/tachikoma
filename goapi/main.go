package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"

	"github.com/BurntSushi/toml"
)

var DBConnection *sql.DB
var AppConfig Config

func main() {
	configDir := os.Getenv("CONFIG_DIR")

	tomlPath := fmt.Sprintf("%s/base.toml", configDir)

	tomlContent, err := os.ReadFile(tomlPath)
	if err != nil {
		fmt.Printf("[GO API] error reading base.toml [configured path: %s]: %v\n", tomlPath, err)
	}

	if _, err := toml.Decode(string(tomlContent), &AppConfig); err != nil {
		fmt.Printf("[GO API] error decoding base.toml: %v\n", err)
	}

	if os.Getenv("APP_ENVIRONMENT") == "production" {
		tomlPath = fmt.Sprintf("%s/production.toml", configDir)
		tomlContent, err = os.ReadFile(tomlPath)
		if err != nil {
			fmt.Printf("[GO API] error reading production.toml [configured path: %s]: %v\n", tomlPath, err)
		}
		if _, err := toml.Decode(string(tomlContent), &AppConfig); err != nil {
			fmt.Printf("[GO API] error decoding production.toml: %v\n", err)
		}
	} else {
		tomlPath = fmt.Sprintf("%s/local.toml", configDir)
		tomlContent, err := os.ReadFile(tomlPath)
		if err != nil {
			fmt.Printf("[GO API] error reading local.toml [configured path: %s]: %v\n", tomlPath, err)
		}
		if _, err := toml.Decode(string(tomlContent), &AppConfig); err != nil {
			fmt.Printf("[GO API] error decoding local.toml: %v\n", err)
		}
	}

	dbConnection, err := connectToDB(DBParams{
		Host:     AppConfig.DBConf.Host,
		Port:     AppConfig.DBConf.Port,
		User:     AppConfig.DBConf.User,
		Password: AppConfig.DBConf.Password,
		DBname:   AppConfig.DBConf.DatabaseName,
	})
	if err != nil {
		log.Fatalf("[GO API] Connection to DB failed: %v\n", err)
	}
	DBConnection = dbConnection

	mux := http.NewServeMux()
	mux.HandleFunc("POST /addhost", addhost)
	mux.HandleFunc("POST /removehost", removehost)
	mux.HandleFunc("POST /status", status)

	fmt.Printf("[GO API] Listening on port %d\n", AppConfig.GoAPI.Port)
	log.Fatal(http.ListenAndServe(fmt.Sprintf(":%d", AppConfig.GoAPI.Port), mux))
}

func status(w http.ResponseWriter, r *http.Request) {
	token := r.Header.Get("Authorization")
	if token == "" {
		http.Error(w, "Missing token", http.StatusUnauthorized)
		return
	}

	if token == AppConfig.GoAPI.Token {
		message := "Service is UP and RUNNING"
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    message,
			Result: "good",
		})
	} else {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    "Invalid token",
			Result: "bad",
		})
	}
}

func addhost(w http.ResponseWriter, r *http.Request) {
	token := r.Header.Get("Authorization")
	if token == "" {
		http.Error(w, "Missing token", http.StatusUnauthorized)
		return
	}

	var result string = "bad"

	if token == AppConfig.GoAPI.Token {

		var req GoAPIRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "Invalid JSON", http.StatusBadRequest)
			return
		}

		var message string
		var resultStatus bool

		e := addNewHostToDB(DBConnection, Params{
			HostGroup: req.HostGroup,
			HostIP:    req.HostIP,
			Hostname:  req.Hostname,
			TableName: "hosts",
		})
		if e != nil {
			message = fmt.Sprintf("Failed to add host [%s] to DB: %v", req.HostIP, e)
			resultStatus = false
		} else {
			message = fmt.Sprintf("Successfully added a new host [%s] to DB", req.HostIP)
			resultStatus = true
		}

		if resultStatus {
			result = "good"
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    message,
			Result: result,
		})
	} else {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    "Invalid token",
			Result: result,
		})
	}
}

func removehost(w http.ResponseWriter, r *http.Request) {
	token := r.Header.Get("Authorization")
	if token == "" {
		http.Error(w, "Missing token", http.StatusUnauthorized)
		return
	}

	var result string = "bad"

	if token == AppConfig.GoAPI.Token {

		var req GoAPIRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "Invalid JSON", http.StatusBadRequest)
			return
		}

		var message string
		var resultStatus bool

		e := deleteHostFromDB(DBConnection, Params{
			HostIP:    req.HostIP,
			TableName: "hosts",
		})
		if e != nil {
			message = fmt.Sprintf("Failed to remove host [%s] from DB: %v", req.HostIP, e)
			resultStatus = false
		} else {
			message = fmt.Sprintf("Successfully removed a host [%s] from DB", req.HostIP)
			resultStatus = true
		}

		if resultStatus {
			result = "good"
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    message,
			Result: result,
		})
	} else {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(GoAPIResponse{
			Msg:    "Invalid token",
			Result: result,
		})
	}
}
