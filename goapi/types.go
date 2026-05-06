package main

// --------------- Configuration ---------------

type Config struct {
	DBConf databaseConfig `toml:"database"`
	GoAPI  goapiConfig    `toml:"goapi"`
}

type databaseConfig struct {
	Host         string `toml:"host"`
	Port         int    `toml:"port"`
	User         string `toml:"username"`
	Password     string `toml:"password"`
	DatabaseName string `toml:"database_name"`
}

type goapiConfig struct {
	Token string `toml:"token"`
	Port  int64  `toml:"port"`
}

// --------------- HTTP handling ---------------

type HostGroup int

type GoAPIRequest struct {
	HostIP    string `json:"hostip"`
	Hostname  string `json:"hostname"`
	HostGroup int    `json:"hostgrp"`
}

type GoAPIResponse struct {
	Result string `json:"result"`
	Msg    string `json:"msg"`
}

// --------------- Other types ---------------

type Params struct {
	Hostname  string
	HostIP    string
	HostGroup int
	TableName string
}

type DBParams struct {
	Host     string
	Port     int
	User     string
	Password string
	DBname   string
}
